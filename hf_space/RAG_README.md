# RAG Implementation - Phase 3

This document describes the basic RAG (Retrieval-Augmented Generation) implementation for the Araliya Bot.

## Overview

The RAG system enhances the chatbot's responses by retrieving relevant context from a knowledge base of blog posts and documents. This implementation follows the **Phase 3** plan from the project documentation.

## Architecture

```
User Query → RAG Service → FAISS Search → Context → OpenAI API → Enhanced Response
```

### Components

1. **RAG Service** (`services/rag_service.py`)
   - Manages FAISS vector index
   - Handles document ingestion and retrieval
   - Uses `sentence-transformers` for embeddings

2. **Sample Data** (`data/sample_posts.py`)
   - Contains sample blog posts for testing
   - Covers topics: GNNs, RAG, Vector DBs, FastAPI, Svelte

3. **Integration** (`api/routes.py`)
   - Modified `/chat` endpoint to include RAG context
   - Added `/rag/status` endpoint for monitoring

## Key Features

- **FAISS Vector Store**: Efficient similarity search
- **Sentence Transformers**: `all-MiniLM-L6-v2` for embeddings
- **Automatic Initialization**: Loads sample data on startup
- **Context Limiting**: Respects token limits for LLM context
- **Graceful Degradation**: Works even if RAG fails

## API Endpoints

### Chat with RAG
```http
POST /api/v1/chat
{
  "message": "What are Graph Neural Networks?",
  "session_id": "optional_session_id"
}
```

### RAG Status
```http
GET /api/v1/rag/status
```

Returns:
```json
{
  "status": "active",
  "total_documents": 5,
  "embedding_model": "all-MiniLM-L6-v2",
  "embedding_dimension": 384
}
```

## Testing

### 1. Test RAG Service Directly
```bash
cd hf_space
python scripts/test_rag.py
```

### 2. Test CPU-Optimized RAG
```bash
cd hf_space
python scripts/test_cpu_rag.py
```

### 3. Test API with RAG
```bash
cd hf_space
python scripts/test_api_rag.py
```

### 3. Manual Testing
Start the server and test with queries like:
- "What are Graph Neural Networks?"
- "How does RAG work?"
- "Tell me about vector databases"

## Configuration

### Dependencies
- `faiss-cpu`: Vector similarity search (CPU-optimized)
- `sentence-transformers`: Text embeddings
- `numpy`: Numerical operations
- `torch`: PyTorch backend (CPU-only)
- `spaces`: HF Spaces GPU acceleration decorator
- `transformers`: Transformer models support

### Files Structure
```
hf_space/app/
├── data/
│   └── sample_posts.py          # Sample knowledge base
├── services/
│   └── rag_service.py           # RAG implementation
└── scripts/
    ├── test_rag.py              # RAG service tests
    └── test_api_rag.py          # API integration tests
```

## How It Works

1. **Startup**: RAG service initializes with sample blog posts
2. **Query Processing**: User query triggers similarity search
3. **Context Retrieval**: Top relevant documents are retrieved
4. **Context Formatting**: Documents formatted for LLM prompt
5. **Enhanced Generation**: OpenAI generates response with context

## Performance

- **Embedding Model**: Lightweight `all-MiniLM-L6-v2` (384 dimensions)
- **Search Speed**: FAISS provides fast similarity search
- **Memory Usage**: CPU-optimized - suitable for HF Spaces
- **GPU Acceleration**: `@spaces.GPU` decorator for HF Spaces
- **CPU Fallback**: Works without GPU for local development
- **Context Limit**: ~2000 tokens to fit within LLM limits

## Next Steps

This basic implementation provides the foundation for:

1. **Real Data Ingestion**: Replace sample data with actual blog feeds
2. **Graph RAG**: Add Neo4j for relationship-based retrieval
3. **Hybrid Search**: Combine vector and keyword search
4. **Automatic Updates**: Periodic knowledge base refresh
5. **Advanced Chunking**: Better document splitting strategies

## Troubleshooting

### Common Issues

1. **Import Errors**: Ensure all dependencies are installed
2. **FAISS Issues**: Check if `faiss-cpu` is properly installed
3. **Memory Issues**: Reduce embedding model size if needed
4. **No Context**: Verify sample data is loaded correctly

### Debugging

Check RAG status:
```bash
curl http://localhost:7860/api/v1/rag/status
```

View logs for RAG initialization:
```bash
# Look for "Initializing RAG service..." in server logs
```

## Success Criteria ✅

- [x] FAISS vector store operational
- [x] Sample documents indexed and searchable
- [x] Chat responses enhanced with retrieved context
- [x] RAG status endpoint functional
- [x] Test scripts passing
- [x] Graceful error handling

The basic RAG implementation is now complete and ready for testing!
