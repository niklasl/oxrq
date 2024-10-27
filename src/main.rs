use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;

use oxigraph::io::{RdfFormat, RdfParser, RdfSerializer};
use oxigraph::model::{GraphName, Quad};
use oxigraph::sparql::results::{QueryResultsFormat, QueryResultsSerializer};
use oxigraph::sparql::{Query, QueryResults, Update};
use oxigraph::store::Store;

fn collect_input(
    args: Vec<String>,
    store: &Store,
    query_str: &mut String,
    prefixes: &mut HashMap<String, String>,
) {
    let loader = store.bulk_loader();

    // Read data from stdin:
    let stdin = std::io::stdin();
    let reader = std::io::BufReader::new(stdin.lock());
    let parser = RdfParser::from_format(RdfFormat::Turtle);

    let mut parser_reader = parser.rename_blank_nodes().for_reader(reader);
    let quads = parser_reader
        .by_ref()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    loader.load_quads(quads).unwrap();

    for (pfx, ns) in parser_reader.prefixes() {
        prefixes.insert(pfx.to_owned(), ns.to_owned());
    }

    // Read data from files:
    if args.len() > 2 {
        for fpath in &args[2..] {
            let fpath = Path::new(fpath);
            let ext = fpath.extension().and_then(OsStr::to_str).unwrap();
            let format = RdfFormat::from_extension(ext).unwrap();
            let parser = RdfParser::from_format(format);

            let file = std::fs::File::open(fpath).unwrap();
            let reader = std::io::BufReader::new(file);
            let mut parser_reader = parser.rename_blank_nodes().for_reader(reader);
            let quads = parser_reader
                .by_ref()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            loader.load_quads(quads).unwrap();
            for (pfx, ns) in parser_reader.prefixes() {
                if !prefixes.contains_key(pfx) {
                    prefixes.insert(pfx.to_owned(), ns.to_owned());
                }
            }
        }
    }

    // Get query body:
    let mut query_body = "";
    if args.len() > 1 {
        query_body = args[1].as_ref();
    }

    // Prepend found prefixes to query:
    for (pfx, ns) in prefixes.iter() {
        query_str.push_str(&format!("PREFIX {pfx}: <{ns}>\n"));
    }
    query_str.push_str(query_body);
}

fn main() {
    let mut store = Store::new().unwrap();
    let mut query_str = String::new();
    let mut prefixes: HashMap<String, String> = HashMap::new();
    let base_iri: Option<&str> = None;

    let args: Vec<_> = std::env::args().collect();
    collect_input(args, &store, &mut query_str, &mut prefixes);

    // Ouput writer:
    let stdout = std::io::stdout();
    let writer = std::io::BufWriter::new(stdout.lock());

    // Run query:
    if let Ok(query) = Query::parse(&query_str, base_iri) {
        match store.query(query).unwrap() {
            QueryResults::Solutions(solutions) => {
                // Select
                let format = QueryResultsFormat::from_extension("tsv").unwrap();
                let mut serializer = QueryResultsSerializer::from_format(format)
                    .serialize_solutions_to_writer(writer, solutions.variables().to_vec())
                    .unwrap();
                for solution in solutions {
                    serializer.serialize(&solution.unwrap()).unwrap();
                }
                return;
            }

            QueryResults::Boolean(result) => {
                // Ask
                let format = QueryResultsFormat::from_extension("tsv").unwrap();
                QueryResultsSerializer::from_format(format)
                    .serialize_boolean_to_writer(writer, result)
                    .unwrap();
                return;
            }

            QueryResults::Graph(triples) => {
                // Construct or Describe
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
        // Insert or Delete
        let update = Update::parse(&query_str, base_iri).unwrap();
        store.update(update).unwrap();
    }

    // Serialize resulting store:
    let mut serializer = RdfSerializer::from_format(RdfFormat::TriG);
    for (pfx, ns) in prefixes {
        serializer = serializer.with_prefix(pfx, ns).unwrap();
    }

    store.dump_to_writer(serializer, writer).unwrap();
}
