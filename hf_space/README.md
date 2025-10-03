---
title: Araliya Bot
emoji: 👁
colorFrom: red
colorTo: blue
sdk: gradio
sdk_version: 5.48.0
app_file: app.py
pinned: false
license: mit
short_description: ai avatar w rag
---

# Araliya Bot - HF Space Backend

**Phase 1: Basic HF ZeroGPU Bot**

This is the backend API for the Araliya Graph-RAG chatbot system, deployed on Hugging Face Spaces. Phase 1 provides a foundational chat API with OpenAI integration and session management.

## Features

- **FastAPI REST API** with automatic documentation
- **OpenAI Integration** for high-quality chat responses
- **Session Management** with conversation context
- **Health Monitoring** with connectivity checks
- **CORS Support** for frontend integration
- **Comprehensive Error Handling**

## API Endpoints

### Core Endpoints

- `GET /` - Root endpoint with API information
- `GET /api/v1/health` - Health check and system status
- `POST /api/v1/chat` - Main chat endpoint
- `GET /api/v1/sessions/{session_id}` - Get session information
- `DELETE /api/v1/sessions/{session_id}` - Clear session history

### Documentation

- `GET /docs` - Interactive API documentation (Swagger UI)
- `GET /redoc` - Alternative API documentation (ReDoc)

## Configuration

The application uses environment variables for configuration:

### Required Variables

- `OPENAI_API_KEY` - Your OpenAI API key (store in HF Secrets)

### Optional Variables

- `OPENAI_MODEL` - OpenAI model to use (default: gpt-3.5-turbo)
- `OPENAI_MAX_TOKENS` - Maximum tokens per response (default: 1000)
- `OPENAI_TEMPERATURE` - Response creativity (default: 0.7)
- `SESSION_TIMEOUT_MINUTES` - Session expiration time (default: 60)
- `MAX_CONVERSATION_HISTORY` - Max messages per session (default: 20)
- `MAX_CONCURRENT_SESSIONS` - Maximum concurrent sessions (default: 10)
- `ALLOWED_ORIGINS` - CORS allowed origins (default: *)

## Deployment on HF Spaces

1. **Create a new HF Space**:
   - Choose "Docker" as the SDK
   - Select appropriate hardware (CPU is sufficient for Phase 1)

2. **Set up environment variables**:
   - Go to Settings → Repository secrets
   - Add `OPENAI_API_KEY` with your OpenAI API key

3. **Upload the code**:
   - Upload all files maintaining the directory structure
   - The space will automatically build and deploy

4. **Test the deployment**:
   - Visit the `/docs` endpoint for interactive testing
   - Use the `/api/v1/health` endpoint to verify connectivity

## Local Development

### Setup

```bash
# Install dependencies
pip install -r requirements.txt

# Set environment variables
export OPENAI_API_KEY="your-api-key-here"

# Run the application
uvicorn app.main:app --reload --port 7860
```

### Testing

```bash
# Test health endpoint
curl http://localhost:7860/api/v1/health

# Test chat endpoint
curl -X POST http://localhost:7860/api/v1/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello, how are you?", "session_id": "test-session"}'
```

## Architecture

```
app/
├── main.py              # FastAPI application entry point
├── config.py            # Configuration management
├── models/              # Pydantic models
│   ├── __init__.py
│   └── chat.py         # Chat request/response models
├── services/            # Business logic services
│   ├── __init__.py
│   ├── openai_service.py    # OpenAI API integration
│   └── session_manager.py   # Session management
└── api/                 # API routes
    ├── __init__.py
    └── routes.py        # API endpoints
```

## Error Handling

The API provides comprehensive error handling:

- **400 Bad Request** - Invalid request format or parameters
- **404 Not Found** - Session or resource not found
- **429 Too Many Requests** - Rate limiting or concurrent session limits
- **500 Internal Server Error** - Server-side errors
- **503 Service Unavailable** - OpenAI API connectivity issues

## Monitoring

- Health checks verify OpenAI connectivity
- Logging provides detailed request/response information
- Session metrics track active conversations
- Response time monitoring for performance

## Security

- API keys stored securely in HF Secrets
- CORS configuration for frontend access control
- Input validation using Pydantic models
- Rate limiting and session management

## Next Steps

Phase 1 establishes the foundation for:

- **Phase 2**: Frontend development with Svelte
- **Phase 3**: RAG implementation with vector databases
- **Phase 4**: Graph-RAG with Neo4j integration

## Support

For issues or questions:
1. Check the `/docs` endpoint for API documentation
2. Review logs in the HF Space interface
3. Verify environment variable configuration
4. Test OpenAI API connectivity independently
