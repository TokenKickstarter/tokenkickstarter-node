#!/bin/bash
set -e
echo "Deploying Node.js Signaling Server with Docker Compose..."

cd /root/worker-cipher/vps-signal-server

# Bring up the containers
docker compose up -d --build

# Verify
docker ps
echo "Deployment complete! Docker mapped 4003 natively to localhost."
