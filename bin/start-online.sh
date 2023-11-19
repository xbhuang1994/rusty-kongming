#!/bin/env bash

# Useage: bin/start-online.sh, bin/start-online.sh daemon

flag=$1

PROJECT_ROOT=$(cd `dirname $0`; cd ..; pwd;)

${PROJECT_ROOT}/bin/sando.sh online $flag