#!/bin/bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}🚀 Pragma Monitoring - Development Setup${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

# Check if .env exists
if [ ! -f .env ]; then
    echo -e "${YELLOW}⚠️  No .env file found. Creating from .env.example...${NC}"
    cp .env.example .env
    echo -e "${YELLOW}   Please edit .env with your configuration (especially APIBARA_API_KEY)${NC}"
    exit 1
fi

# Check if docker is running
if ! docker info > /dev/null 2>&1; then
    echo -e "${RED}❌ Docker is not running. Please start Docker Desktop.${NC}"
    exit 1
fi

echo -e "${GREEN}✅ Docker is running${NC}"

# Check if pragma-node repo exists
if [ ! -d "../pragma-node/sql" ]; then
    echo -e "${YELLOW}⚠️  pragma-node repository not found at ../pragma-node${NC}"
    echo -e "${YELLOW}   For full setup, clone it next to this repository:${NC}"
    echo -e "${YELLOW}   git clone https://github.com/astraly-labs/pragma-node ../pragma-node${NC}"
fi

# Start docker-compose services
echo -e "${BLUE}📦 Starting PostgreSQL/TimescaleDB and OTEL services...${NC}"
docker-compose up -d

# Wait for PostgreSQL to be ready
echo -e "${BLUE}⏳ Waiting for PostgreSQL to be ready...${NC}"
until docker exec pragma-postgres pg_isready -U postgres > /dev/null 2>&1; do
    sleep 1
done
echo -e "${GREEN}✅ PostgreSQL is ready${NC}"

# Run migrations from pragma-node if available
if [ -d "../pragma-node/sql" ]; then
    echo -e "${BLUE}🗄️  Running database migrations from pragma-node/sql...${NC}"
    for sql_file in ../pragma-node/sql/*.sql; do
        echo -e "   Running $(basename $sql_file)..."
        docker exec -i pragma-postgres psql -U postgres -d pragma_monitoring < "$sql_file" 2>/dev/null || true
    done
    echo -e "${GREEN}✅ Migrations complete${NC}"
else
    echo -e "${YELLOW}⚠️  Skipping migrations (pragma-node not found)${NC}"
fi

echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}🎉 Development environment ready!${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo -e "📊 Services:"
echo -e "   • PostgreSQL/TimescaleDB: ${GREEN}localhost:5432${NC}"
echo -e "   • Grafana:    ${GREEN}http://localhost:3000${NC} (admin/admin)"
echo -e "   • OTLP gRPC:  ${GREEN}localhost:4317${NC}"
echo -e "   • OTLP HTTP:  ${GREEN}localhost:4318${NC}"
echo ""
echo -e "🚀 To start the monitoring service:"
echo -e "   ${YELLOW}cargo run${NC}"
echo ""
echo -e "📋 To view logs in Grafana:"
echo -e "   1. Open ${GREEN}http://localhost:3000${NC}"
echo -e "   2. Go to Explore > Select 'Loki' data source"
echo -e "   3. Query: {service_name=\"pragma-monitoring\"}"
echo ""
echo -e "🛑 To stop services:"
echo -e "   ${YELLOW}docker-compose down${NC}"
echo ""
