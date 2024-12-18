# OxRQ

OxRQ is a simple command-line tool (`oxrq`) for running SPARQL queries over a stream of RDF data or a set of RDF files. It uses [Oxigraph](https://github.com/oxigraph/oxigraph) for all functionality.

## Purpose

This tool is primarily made for working with "one-off" queries over a set of RDF source files on the command-line. It can be used to query, edit and/or create new RDF data using SPARQL.

(It is loosely inspired by the workflow of [AWK](https://en.wikipedia.org/wiki/AWK), and in some ways aims to be an alternative to the venerable [`rasqal`](https://librdf.org/rasqal/roqet.html) and [`rapper`](https://librdf.org/raptor/rapper.html) tools.)

## Install

For now, check out this repository and use [Cargo](https://doc.rust-lang.org/cargo):

    $ cargo install --path .

## Example Usage

```console
$ echo '
PREFIX : <http://example.org/ns#>
BASE <http://example.org/>
<item/1> a :Item ; :name "Item 1" .
' > /tmp/test_oxrq.ttl

$ cat /tmp/test_oxrq.ttl | oxrq 'construct { ?item a :Thing } { ?item a :Item }'
@prefix : <http://example.org/ns#> .
<http://example.org/item/1> a :Thing .

$ oxrq 'insert { ?item :name "Item One" } where { ?item :name "Item 1" }' /tmp/test_oxrq.ttl
@prefix : <http://example.org/ns#> .
<http://example.org/item/1> :name "Item One" , "Item 1" ;
	a :Item .

$ oxrq 'select ?s ?p ?o { ?s ?p ?o filter(?p = :name) }' /tmp/test_oxrq.ttl
?s	?p	?o
<http://example.org/item/1>	<http://example.org/ns#name>	"Item 1"

$ oxrq /tmp/test_oxrq.ttl -fo rdf > /tmp/test_oxrq.rdf
$ cat /tmp/test_oxrq.rdf | oxrq -irdf 'select(count(*)as?count){?s?p?o}' -ocsv
count
2
```

## Usage Details

The `oxrq` command reads RDF from stdin (TriG by default, use `--input-format` (or `-i`) to change), and executes the SPARQL query provided as the first argument.

Prefixes used in the source data will be prepended to the SPARQL query, and will be used when serializing (if possible). First found prefix takes precedence, so an empty RDF file can be used to set preferred prefixes.

If file arguments are provided, those are read as input data files instead (format detected by suffix), into a named graph named by the file IRI. If file arguments are passed, stdin will only be read if the special `-` name is used.

If `--file-query` (or `-f`) is given, the first argument will be treated as the other input files, and any file with an `.rq` suffix will be read from as the query (if multiple query files are given, only the last one will be used).

Output format is controlled with `--output-format` (or `-o`). TriG is used by default, giving Turtle compatible output for `CONSTRUCT` or `DESCRIBE` (as one new graph). `INSERT` or `DELETE` updates modify input data (but not source files). TSV is used for `SELECT` and `ASK`.

(The combination `-f -o FORMAT` is useful to reformat data, e.g. `oxrq some.rdf -fo ttl > some.ttl`.)

For formats that cannot serialize datasets, the default graph will be serialized unless empty, in which case an arbitrary named graph will be chosen. (This only works predictably for one input file; use `CONSTRUCT` queries for full control.)

To prevent reading from stdin, use `--no-stdin` (or `-n`). This is useful when creating RDF using self-contained `CONSTRUCT` queries containing `VALUES` clauses.
