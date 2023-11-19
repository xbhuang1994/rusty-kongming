#!/bin/env bash

# Useage: bin/start-debug.sh, bin/start-debug.sh daemon

flag=$1

PROJECT_ROOT=$(cd `dirname $0`; cd ..; pwd;)

${PROJECT_ROOT}/bin/sando.sh debug $flag