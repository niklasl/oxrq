use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

use anyhow::{bail, Context, Result};
use clap::Parser as CliParser;

use oxigraph::io::{RdfFormat, RdfParser, RdfSerializer};
use oxigraph::model::{GraphName, GraphNameRef, NamedNode, Quad};
use oxigraph::sparql::results::{QueryResultsFormat, QueryResultsSerializer};
use oxigraph::sparql::{Query, QueryResults, SparqlSyntaxError, Update};
use oxigraph::store::{BulkLoader, Store};

#[derive(CliParser)]
#[command(version, about, long_about = None)]
struct CliArgs {
    /// Input RDF format (ttl, rdf, nt, nq)
    #[arg(short, long)]
    input_format: Option<String>,

    /// Output RDF format (ttl, rdf, nt, nq) or SPARQL results format (tsv, csv, json, xml)
    #[arg(short, long)]
    output_format: Option<String>,

    /// Base IRI used when parsing
    #[arg(short, long)]
    base_iri: Option<String>,

    /// Provide query via file (with '.rq' suffix)
    #[arg(short, long)]
    file_query: bool,

    /// Do not read from stdin (unless '-' is given as file)
    #[arg(short, long)]
    no_stdin: bool,

    /// Query string (unless '--file-query' is used)
    query: Option<String>,

    /// RDF file(s)
    file: Vec<String>,
}

fn collect_input(
    args: &mut CliArgs,
    store: &Store,
    query_str: &mut String,
    base_iri: &mut Option<String>,
    prefixes: &mut HashMap<String, String>,
) -> Result<()> {
    if let Some(value) = &args.base_iri {
        base_iri.get_or_insert(value.to_owned());
    }

    // Use query as file:
    if args.file_query {
        if let Some(actually_fpath) = &args.query {
            args.file.push(actually_fpath.to_owned());
            args.query = None;
        }
    }

    let mut use_stdin = !args.no_stdin && args.file.len() == 0;

    let mut query_file: Option<&str> = None;

    let loader = store.bulk_loader();

    // Read data from files:
    for fpath in &args.file {
        if fpath == "-" {
            use_stdin = true;
            continue;
        }

        let path = Path::new(fpath);
        let ext = path
            .extension()
            .and_then(OsStr::to_str)
            .with_context(|| format!("Needs file extensions to detect input format"))?;

        if ext == "rq" {
            query_file = Some(fpath);
            continue;
        }

        let format = RdfFormat::from_extension(ext)
            .with_context(|| format!("No RDF format found for extension {ext}"))?;

        let file = File::open(path).with_context(|| format!("Unable to open file: {fpath}"))?;
        let reader = BufReader::new(file);

        // Use file path as named graph IRI
        let graph_iri = if fpath.starts_with("/") {
            format!("file://{fpath}")
        } else {
            format!("file:{fpath}")
        }
        .replace(" ", "%20");

        let parser = RdfParser::from_format(format)
            .with_default_graph(NamedNode::new(&graph_iri)?)
            .with_base_iri(base_iri.as_ref().unwrap_or(&graph_iri))?;

        if let Err(e) = load_data(&loader, parser, reader, base_iri, prefixes) {
            eprintln!("Error in file '{fpath}': {e}");
            continue;
        }
    }

    // Read data from stdin:
    if use_stdin {
        let format = if let Some(fmt) = &args.input_format {
            RdfFormat::from_extension(&fmt)
                .with_context(|| format!("Unknown input format: {fmt}"))?
        } else {
            RdfFormat::Turtle
        };
        let stdin = std::io::stdin();
        let reader = BufReader::new(stdin.lock());

        let mut parser = RdfParser::from_format(format);
        if let Some(value) = base_iri {
            parser = parser.with_base_iri(value.to_owned())?;
        }

        load_data(&loader, parser, reader, base_iri, prefixes)?;
    }

    // Get query:
    if let Some(fpath) = query_file {
        let path = Path::new(&fpath);
        let mut file =
            File::open(path).with_context(|| format!("Unable to open query file: {fpath}"))?;
        file.read_to_string(query_str)?;
    } else if let Some(query_body) = &args.query {
        // Prepend found prefixes to query:
        for (pfx, ns) in prefixes.iter() {
            query_str.push_str(&format!("PREFIX {pfx}: <{ns}>\n"));
        }
        // Get query body:
        query_str.push_str(&format!("{}", query_body));
    }

    Ok(())
}

fn load_data<R: Read>(
    loader: &BulkLoader,
    parser: RdfParser,
    reader: BufReader<R>,
    base_iri: &mut Option<String>,
    prefixes: &mut HashMap<String, String>,
) -> Result<()> {
    let mut parser_reader = parser.rename_blank_nodes().for_reader(reader);
    let quads = parser_reader.by_ref().collect::<Result<Vec<_>, _>>()?;

    loader.load_quads(quads)?;

    if let Some(value) = parser_reader.base_iri() {
        base_iri.get_or_insert(value.to_owned());
    }

    for (pfx, ns) in parser_reader.prefixes() {
        if !prefixes.contains_key(pfx) {
            prefixes.insert(pfx.to_owned(), ns.to_owned());
        }
    }

    Ok(())
}

fn query_to_new_store_or_serialize<W: Write>(
    store: &Store,
    mut query: Query,
    output_format: &Option<String>,
    writer: W,
) -> Result<Option<Store>> {
    query.dataset_mut().set_default_graph_as_union();
    let results = store.query(query).context("Query failed")?;
    match results {
        // Select:
        QueryResults::Solutions(solutions) => {
            let format = get_queryresults_format(output_format)?;
            let mut serializer = QueryResultsSerializer::from_format(format)
                .serialize_solutions_to_writer(writer, solutions.variables().to_vec())?;
            for solution in solutions {
                serializer.serialize(&solution?)?;
            }
            // Done serializing:
            return Ok(None);
        }

        // Ask:
        QueryResults::Boolean(result) => {
            let format = get_queryresults_format(output_format)?;
            QueryResultsSerializer::from_format(format)
                .serialize_boolean_to_writer(writer, result)?;
            // Done serializing:
            return Ok(None);
        }

        // Construct or Describe:
        QueryResults::Graph(triples) => {
            let store = Store::new()?;
            for triple in triples {
                let triple = triple?;
                let quad = Quad {
                    subject: triple.subject,
                    predicate: triple.predicate,
                    object: triple.object,
                    graph_name: GraphName::DefaultGraph,
                };
                store.insert(quad.as_ref())?;
            }
            return Ok(Some(store));
        }
    }
}

fn get_queryresults_format(output_format: &Option<String>) -> Result<QueryResultsFormat> {
    if let Some(fmt) = output_format {
        QueryResultsFormat::from_extension(&fmt)
            .with_context(|| format!("Unknown query results format: {fmt}"))
    } else {
        Ok(QueryResultsFormat::Tsv)
    }
}

fn main() -> Result<()> {
    let mut store = Store::new()?;
    let mut query_str = String::new();
    let mut prefixes: HashMap<String, String> = HashMap::new();
    let mut base_iri: Option<String> = None;

    let mut args = CliArgs::parse();

    collect_input(
        &mut args,
        &store,
        &mut query_str,
        &mut base_iri,
        &mut prefixes,
    )?;

    // Output:
    let stdout = std::io::stdout();

    let mut query_parse_err: Option<SparqlSyntaxError> = None;

    // Run query:
    match Query::parse(&query_str, base_iri.as_deref()) {
        Ok(query) => {
            let writer = BufWriter::new(stdout.lock());
            match query_to_new_store_or_serialize(&store, query, &args.output_format, writer)? {
                Some(new_store) => {
                    store = new_store;
                }
                None => {
                    return Ok(());
                }
            }
        }
        Err(err) => {
            query_parse_err = Some(err);
        }
    }

    if let Some(query_parse_err) = query_parse_err {
        // Maybe an update query:
        if let Ok(update) = Update::parse(&query_str, base_iri.as_deref()) {
            // Insert or Delete:
            store.update(update).context("Update failed")?;
        } else {
            // Bail for query error (assumed more likely than update attempt; maybe report both?):
            bail!(query_parse_err);
        }
    }

    let format = if let Some(fmt) = &args.output_format {
        RdfFormat::from_extension(&fmt).with_context(|| format!("Unknown output format: {fmt}"))?
    } else {
        RdfFormat::TriG
    };

    // Serialize resulting store:
    let mut serializer = RdfSerializer::from_format(format);
    for (pfx, ns) in prefixes {
        serializer = serializer.with_prefix(pfx, ns)?;
    }

    let writer = BufWriter::new(stdout.lock());
    if !format.supports_datasets() {
        if store
            .quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph))
            .peekable()
            .peek()
            .is_some()
        {
            store.dump_graph_to_writer(GraphNameRef::DefaultGraph, format, writer)?;
        } else {
            // Picks one named graph at random (i.e. only predictable for one input file):
            for graph_name in store.named_graphs() {
                store.dump_graph_to_writer(graph_name.unwrap().as_ref(), format, writer)?;
                break;
            }
        }
    } else {
        store.dump_to_writer(serializer, writer)?;
    }

    Ok(())
}
