# OxRQ

OxRQ is a simple command-line tool (`oxrq`) for running SPARQL queries over a stream of RDF data or a set of RDF files. It uses [Oxigraph](https://github.com/oxigraph/oxigraph) for all functionality.

## Purpose

This tool is made for conveniently working with "one-off" queries in a command-line centric environment over a (reasonably small) set of RDF source files. It can also be used to query, edit or create new RDF data, and to test queries and generally experiment with SPARQL.

(It is loosely inspired by [AWK](https://en.wikipedia.org/wiki/AWK) and its common workflow.)

Prefixes used in the source data will be prepended to the SPARQL query, and will be used when serializing (if possible).

## Install

For now, check out this repository and use [Cargo](https://doc.rust-lang.org/cargo):

    $ cargo install --path .

## Examples

```console
$ echo '
PREFIX : <http://example.org/ns#>
BASE <http://example.org/>
<item/1> a :Item ; :name "Item 1" .' > /tmp/test.ttl

$ cat /tmp/test.ttl | oxrq 'construct { ?item a :Thing } { ?item a :Item }'
@prefix : <http://example.org/ns#> .
<http://example.org/item/1> a :Thing .

$ oxrq 'insert { ?item :name "Item One" } WHERE { ?item :name "Item 1" }' /tmp/test.ttl
@prefix : <http://example.org/ns#> .
<http://example.org/item/1> :name "Item One" , "Item 1" ;
	a :Item .

$ oxrq 'select ?s ?p ?o { ?s ?p ?o filter(?p = :name) }' /tmp/test.ttl
?s	?p	?o
<http://example.org/item/1>	<http://example.org/ns#name>	"Item 1"
```
