#!/bin/sh
# Run bridge and config server together
# Bridge runs in background, config server in foreground
# If either exits, the other gets killed too

# Use satellite test config if no config exists yet
if [ ! -f /etc/rpi-osc-bridge/config.json ]; then
    cp config.satellite-test.json /etc/rpi-osc-bridge/config.json
fi

python3 bridge.py &
BRIDGE_PID=$!

trap "kill $BRIDGE_PID 2>/dev/null; exit" INT TERM

python3 config_server.py &
CONFIG_PID=$!

wait -n
kill $BRIDGE_PID $CONFIG_PID 2>/dev/null
