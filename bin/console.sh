#!/bin/env bash

# goto project root
PROJECT_ROOT=$(cd `dirname $0`; cd ..; pwd;)
# goto bot folder
cd ${PROJECT_ROOT}/bot;

ADDR=$1
cargo run --release --bin op-sidecar -- console ${ADDR}