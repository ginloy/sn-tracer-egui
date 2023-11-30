#!/usr/bin/env bash

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
export SCANNER_PATH="${DIR}/scanner"
"${DIR}/sn-tracer-egui" $@