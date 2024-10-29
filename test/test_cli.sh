#!/bin/sh
cd $(dirname $0)

test() {
    echo "# $1"
    cat resources/file1.ttl | oxrq "$1"
    echo
}

test 'construct { ?item a :Thing } { ?item a :Item }'

test 'delete { ?item :name ?name } WHERE { ?item :name ?name }'

test 'insert { ?item :name "Item One" } WHERE { ?item :name "Item 1" }'

test 'select ?s ?p ?o { ?s ?p ?o }'

test 'ask { ?item a :Item }'
test 'ask { ?item a :NoItem }'

echo "# Query from file"
cat resources/file1.ttl | oxrq -f resources/query1.rq
echo

echo "# Query from file"
oxrq -f resources/query1.rq resources/file1.ttl
echo

echo "# Output CSV"
oxrq -f resources/query1.rq resources/file1.ttl -ocsv
echo

echo "# Read RDF/XML"
oxrq "select ?s ?p ?o { ?s ?p ?o }" resources/file1.rdf
echo

echo "# Read RDF/XML from stdin"
cat resources/file1.rdf | oxrq -irdf "select ?s ?p ?o { ?s ?p ?o }"
echo

echo "# Output RDF/XML"
oxrq resources/file1.ttl -fo rdf
echo

echo "# Use prefixes from empty file, then read from stdin"
cat resources/file1.ttl | oxrq -onq | oxrq -f resources/file0.ttl -
echo

echo "# construct from values"
oxrq -n 'prefix : <https://example.org/vocab/>
        base <https://example.org/>
        construct { ?item a ?type; :name ?name } {
          values (?item ?type ?name) {
            (<item/1> :Item "Item One")
            (<item/2> :Item "Item Two")
          }
        }'
