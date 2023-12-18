#!/usr/bin/env bash
set -ex
cd "$(dirname "$0")"

docker rm -f oracle_monitoring
docker run -d --name=oracle_monitoring -p 5432:5432 -e POSTGRES_PASSWORD=password postgres
sleep 5

DATABASE_URL=postgresql://postgres:password@localhost:5432/test cargo test