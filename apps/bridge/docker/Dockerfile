FROM python:3.11-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    gcc libc6-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /opt/rpi-osc-bridge

COPY apps/bridge/src/rpi_osc_bridge/*.py ./
COPY apps/bridge/web/ ./web/
COPY apps/bridge/config/config.example.json ./
COPY apps/bridge/tests/ ./tests/

RUN pip install --no-cache-dir evdev python-osc zeroconf pytest

# Create runtime dirs
RUN mkdir -p /etc/rpi-osc-bridge /var/run/rpi-osc-bridge /var/log \
    && cp config.example.json /etc/rpi-osc-bridge/config.json \
    && touch /var/log/rpi-osc-bridge.log

EXPOSE 80 9002

CMD ["python3", "config_server.py"]
