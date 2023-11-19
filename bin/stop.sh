#!/bin/env bash

# goto project root
PROJECT_ROOT=$(cd `dirname $0`; cd ..; pwd;)

# check if exists pid
PID_FILE=${PROJECT_ROOT}/bin/sando.pid
if [[ -e ${PID_FILE} ]]; then
    PID=`cat ${PID_FILE}`
else
    PID=""
fi

# check really run already
if [[ -n ${PID} ]]; then
    EXISTS_PID=$(ps -ax | grep sando | grep ${PID} | grep -v grep | awk '{print $1}')

    if [[ -n ${EXISTS_PID} && ${EXISTS_PID} = ${PID} ]]; then
        kill -15 ${EXISTS_PID}
        echo "Sando has stopped."
    fi
    # clear invalid pid file
    rm -f ${PID_FILE}
else
    echo "Sando is not running."
fi