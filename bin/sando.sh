#!/bin/env bash

p1=$1
p2=$2
if [[ -z $p1 ]]; then
    echo "Usage Sample: ./sando.sh test or use start-debug.sh / start-online.sh"
    echo "Tips: add parameter 'daemon' for run sando in background."
    exit 0;
fi

# goto project root
PROJECT_ROOT=$(cd `dirname $0`; cd ..; pwd;)
# echo ${PROJECT_ROOT}

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
        echo "Sando is already running."
        exit 0;
    else
        # remove invalid pid file
        rm -f ${PID_FILE}
    fi
fi

# goto bot folder
cd ${PROJECT_ROOT}/bot;

if [[ $p1 = "debug" ]]; then
    flag=" --features debug "
elif [[ $p1 = "online" ]]; then
    flag=""
else
    echo "Only 'debug' or 'online' is allown."
    exit 0;
fi

if [[ $p2 = "daemon" ]]; then
    nohup cargo run --release --bin rusty-sando ${flag} > /dev/null 2>&1 &
    pid=$!
    echo ${pid} > ${PID_FILE}
    echo "Sando started.";
else
    pid=$$
    echo ${pid} > ${PID_FILE}
    exec cargo run --release --bin rusty-sando ${flag}
fi