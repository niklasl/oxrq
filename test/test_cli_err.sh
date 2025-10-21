#!/usr/bin/env bash
cd `dirname $0`

echo "# Error in file, continue with next"
oxrq 'ask {?s ?p ?o}' resources/err1.ttl resources/file1.ttl
echo
