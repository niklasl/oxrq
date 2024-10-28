use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read};
use std::path::Path;

use clap::Parser as CliParser;

use oxigraph::io::{RdfFormat, RdfParser, RdfSerializer, ReaderQuadParser};
use oxigraph::model::{GraphName, GraphNameRef, Quad};
use oxigraph::sparql::results::{QueryResultsFormat, QueryResultsSerializer};
use oxigraph::sparql::{Query, QueryResults, Update};
use oxigraph::store::Store;

#[derive(CliParser)]
#[command(version, about, name = "oxrq")]
struct CliArgs {
    #[arg(short, long)]
    input_format: Option<String>,

    #[arg(short, long)]
    output_format: Option<String>,

    #[arg(short, long)]
    base_iri: Option<String>,

    #[arg(short, long)]
    file_query: bool,

    #[arg(short, long)]
    no_stdin: bool,

    query: Option<String>,
    file: Vec<String>,
}

fn collect_input(
    args: &mut CliArgs,
    store: &Store,
    query_str: &mut String,
    base_iri: &mut Option<String>,
    prefixes: &mut HashMap<String, String>,
) {
    let loader = store.bulk_loader();

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

    let mut use_stin = !args.no_stdin && args.file.len() == 0;

    let mut query_file: Option<&str> = None;

    // Read data from files:
    for fpath in &args.file {
        if fpath == "-" {
            use_stin = true;
            continue
        }

        let path = Path::new(fpath);
        let ext = path.extension().and_then(OsStr::to_str).unwrap();

        if ext == "rq" {
            query_file = Some(fpath);
            continue
        }

        let format = RdfFormat::from_extension(ext).unwrap();
        let parser = RdfParser::from_format(format);

        let file = File::open(path).unwrap();
        let reader = BufReader::new(file);
        let mut parser_reader = parser.rename_blank_nodes().for_reader(reader);
        let quads = parser_reader
            .by_ref()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        loader.load_quads(quads).unwrap();

        opt_set_base_iri_and_prefixes(&parser_reader, base_iri, prefixes);
    }

    // Read data from stdin:
    if use_stin {
        let stdin = std::io::stdin();
        let reader = BufReader::new(stdin.lock());
        let format = if let Some(fmt) = &args.input_format {
            RdfFormat::from_extension(&fmt).unwrap()
        } else {
            RdfFormat::Turtle
        };
        let parser = RdfParser::from_format(format);

        let mut parser_reader = parser.rename_blank_nodes().for_reader(reader);
        let quads = parser_reader
            .by_ref()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        loader.load_quads(quads).unwrap();

        opt_set_base_iri_and_prefixes(&parser_reader, base_iri, prefixes);
    }

    // Get query:
    if let Some(fpath) = query_file {
        let fpath = Path::new(&fpath);
        let mut file = File::open(fpath).unwrap();
        file.read_to_string(query_str).unwrap();
    } else if let Some(query_body) = &args.query {
        // Prepend found prefixes to query:
        for (pfx, ns) in prefixes.iter() {
            query_str.push_str(&format!("PREFIX {pfx}: <{ns}>\n"));
        }
        // Get query body:
        query_str.push_str(&format!("{}", query_body));
    }
}

fn opt_set_base_iri_and_prefixes<R: Read>(
    parser_reader: &ReaderQuadParser<R>,
    base_iri: &mut Option<String>,
    prefixes: &mut HashMap<String, String>,
) {
    if let Some(value) = parser_reader.base_iri() {
        base_iri.get_or_insert(value.to_owned());
    }

    for (pfx, ns) in parser_reader.prefixes() {
        if !prefixes.contains_key(pfx) {
            prefixes.insert(pfx.to_owned(), ns.to_owned());
        }
    }
}

fn main() {
    let mut store = Store::new().unwrap();
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
    );

    // Ouput writer:
    let stdout = std::io::stdout();
    let writer = BufWriter::new(stdout.lock());

    // Run query:
    if let Ok(query) = Query::parse(&query_str, base_iri.as_deref()) {
        let results = store.query(query).unwrap();
        match results {
            // Select:
            QueryResults::Solutions(solutions) => {
                let format = if let Some(fmt) = &args.output_format {
                    QueryResultsFormat::from_extension(&fmt).unwrap()
                } else {
                    QueryResultsFormat::Tsv
                };
                let mut serializer = QueryResultsSerializer::from_format(format)
                    .serialize_solutions_to_writer(writer, solutions.variables().to_vec())
                    .unwrap();
                for solution in solutions {
                    serializer.serialize(&solution.unwrap()).unwrap();
                }
                // Done serializing:
                return;
            }

            // Ask:
            QueryResults::Boolean(result) => {
                let format = if let Some(fmt) = &args.output_format {
                    QueryResultsFormat::from_extension(&fmt).unwrap()
                } else {
                    QueryResultsFormat::Tsv
                };
                QueryResultsSerializer::from_format(format)
                    .serialize_boolean_to_writer(writer, result)
                    .unwrap();
                // Done serializing:
                return;
            }

            // Construct or Describe:
            QueryResults::Graph(triples) => {
                store = Store::new().unwrap();
                for triple in triples {
                    let triple = triple.unwrap();
                    let quad = Quad {
                        subject: triple.subject,
                        predicate: triple.predicate,
                        object: triple.object,
                        graph_name: GraphName::DefaultGraph,
                    };
                    store.insert(quad.as_ref()).unwrap();
                }
            }
        }
    } else {
        // Insert or Delete:
        let update = Update::parse(&query_str, base_iri.as_deref()).unwrap();
        store.update(update).unwrap();
    }

    let format = if let Some(fmt) = &args.output_format {
        RdfFormat::from_extension(&fmt).unwrap()
    } else {
        RdfFormat::TriG
    };

    // Serialize resulting store:
    let mut serializer = RdfSerializer::from_format(format);
    for (pfx, ns) in prefixes {
        serializer = serializer.with_prefix(pfx, ns).unwrap();
    }

    if !format.supports_datasets() {
        store.dump_graph_to_writer(GraphNameRef::DefaultGraph, format, writer).unwrap();
    } else {
        store.dump_to_writer(serializer, writer).unwrap();
    }
}
