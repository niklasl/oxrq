#!/usr/bin/env bash
cd `dirname $0`

echo 'Test CLI'
diff <(./test_cli_out.sh) results/test_cli_out.out
echo 'OK if no diff'
echo

echo 'Test CLI err'
diff <(./test_cli_err.sh 2>&1) results/test_cli_err.out
echo 'OK if no diff'
echo
