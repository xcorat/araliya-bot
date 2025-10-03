#!/bin/bash
# Local testing script for Araliya Bot

set -e

echo "🚀 Starting Araliya Bot Local Tests"
echo "=================================="

# Check if we're in the right directory
if [ ! -f "app/main.py" ]; then
    echo "❌ Error: Please run this script from the hf_space directory"
    exit 1
fi

# Check if OpenAI API key is set
if [ -z "$OPENAI_API_KEY" ]; then
    echo "❌ Error: OPENAI_API_KEY environment variable is not set"
    echo "Please set it with: export OPENAI_API_KEY='your-api-key-here'"
    exit 1
fi

echo "✅ Environment check passed"

# Install dependencies if needed
echo "📦 Installing dependencies..."
pip install -r requirements.txt > /dev/null 2>&1

# Run unit tests
echo "🧪 Running unit tests..."
python -m pytest tests/ -v --tb=short

# Start the server in background
echo "🌐 Starting local server..."
uvicorn app.main:app --host 0.0.0.0 --port 7860 > server.log 2>&1 &
SERVER_PID=$!

# Wait for server to start
echo "⏳ Waiting for server to start..."
sleep 5

# Check if server is running
if ! kill -0 $SERVER_PID 2>/dev/null; then
    echo "❌ Server failed to start. Check server.log for details:"
    cat server.log
    exit 1
fi

echo "✅ Server started successfully (PID: $SERVER_PID)"

# Run deployment tests
echo "🔍 Running deployment tests..."
python scripts/test_deployment.py http://localhost:7860

# Cleanup
echo "🧹 Cleaning up..."
kill $SERVER_PID 2>/dev/null || true
wait $SERVER_PID 2>/dev/null || true

echo "✅ All tests completed successfully!"
echo "🚀 Your Araliya Bot is ready for deployment!"
