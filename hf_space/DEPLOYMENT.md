# Araliya Bot - Deployment Guide

This guide covers deploying the Araliya Bot Phase 1 backend to Hugging Face Spaces.

## Prerequisites

- Hugging Face account
- OpenAI API key
- Git (for code upload)

## Deployment Steps

### 1. Create Hugging Face Space

1. Go to [Hugging Face Spaces](https://huggingface.co/spaces)
2. Click "Create new Space"
3. Configure the space:
   - **Space name**: `araliya-bot` (or your preferred name)
   - **License**: Apache 2.0 (recommended)
   - **SDK**: Docker
   - **Hardware**: CPU Basic (sufficient for Phase 1)
   - **Visibility**: Public or Private (your choice)

### 2. Set Environment Variables

1. Go to your space's **Settings** tab
2. Navigate to **Repository secrets**
3. Add the following secrets:

   ```
   OPENAI_API_KEY = your-openai-api-key-here
   ```

   Optional environment variables (with defaults):
   ```
   OPENAI_MODEL = gpt-3.5-turbo
   OPENAI_MAX_TOKENS = 1000
   OPENAI_TEMPERATURE = 0.7
   SESSION_TIMEOUT_MINUTES = 60
   MAX_CONVERSATION_HISTORY = 20
   MAX_CONCURRENT_SESSIONS = 10
   ALLOWED_ORIGINS = *
   LOG_LEVEL = INFO
   ```

### 3. Upload Code

#### Option A: Git Clone and Push
```bash
# Clone your HF Space repository
git clone https://huggingface.co/spaces/YOUR_USERNAME/araliya-bot
cd araliya-bot

# Copy the application files
cp -r /path/to/hf_space/* .

# Commit and push
git add .
git commit -m "Initial deployment of Araliya Bot Phase 1"
git push
```

#### Option B: Direct File Upload
1. Use the HF Spaces web interface
2. Upload all files maintaining the directory structure:
   ```
   app/
   ├── main.py
   ├── config.py
   ├── models/
   ├── services/
   ├── api/
   └── utils/
   requirements.txt
   Dockerfile
   README.md
   ```

### 4. Verify Deployment

1. Wait for the space to build (usually 2-5 minutes)
2. Check the build logs for any errors
3. Once running, test the endpoints:

   **Health Check:**
   ```bash
   curl https://YOUR_USERNAME-araliya-bot.hf.space/api/v1/health
   ```

   **Chat Test:**
   ```bash
   curl -X POST https://YOUR_USERNAME-araliya-bot.hf.space/api/v1/chat \
     -H "Content-Type: application/json" \
     -d '{"message": "Hello, how are you?"}'
   ```

4. Visit the interactive documentation:
   - Swagger UI: `https://YOUR_USERNAME-araliya-bot.hf.space/docs`
   - ReDoc: `https://YOUR_USERNAME-araliya-bot.hf.space/redoc`

## Troubleshooting

### Common Issues

#### Build Failures
- **Dependency conflicts**: Check requirements.txt versions
- **Python version**: Ensure Dockerfile uses Python 3.9+
- **Missing files**: Verify all application files are uploaded

#### Runtime Errors
- **OpenAI API key**: Verify the key is set in HF Secrets
- **Import errors**: Check file structure and __init__.py files
- **Port issues**: Ensure the app runs on port 7860

#### API Errors
- **CORS issues**: Check ALLOWED_ORIGINS setting
- **Timeout errors**: Verify OpenAI API connectivity
- **Rate limiting**: Monitor OpenAI API usage

### Debugging Steps

1. **Check build logs** in the HF Space interface
2. **Review application logs** for runtime errors
3. **Test locally** before deploying:
   ```bash
   cd hf_space
   pip install -r requirements.txt
   export OPENAI_API_KEY="your-key"
   uvicorn app.main:app --reload --port 7860
   ```

4. **Validate configuration**:
   ```bash
   curl http://localhost:7860/api/v1/health
   ```

## Performance Optimization

### For Free Tier
- Use CPU Basic hardware (sufficient for Phase 1)
- Monitor session limits (default: 10 concurrent)
- Implement request timeouts (default: 30 seconds)

### For Production
- Consider upgrading to CPU Upgrade for better performance
- Monitor OpenAI API costs and usage
- Implement rate limiting if needed

## Security Considerations

1. **API Keys**: Always use HF Secrets, never commit keys to code
2. **CORS**: Configure specific origins instead of "*" for production
3. **Rate Limiting**: Monitor and limit API usage as needed
4. **Logging**: Avoid logging sensitive information

## Monitoring

### Health Checks
- Use `/api/v1/health` for automated monitoring
- Monitor OpenAI connectivity status
- Track response times and error rates

### Logging
- Application logs are available in the HF Space interface
- Monitor for OpenAI API errors and rate limits
- Track session creation and cleanup

### Metrics to Watch
- Active session count
- OpenAI API token usage
- Response times
- Error rates

## Next Steps

After successful Phase 1 deployment:

1. **Phase 2**: Frontend development with Svelte
2. **Phase 3**: RAG implementation with vector databases
3. **Phase 4**: Graph-RAG with Neo4j integration

## Support

For deployment issues:
1. Check HF Spaces documentation
2. Review application logs
3. Test endpoints with provided curl commands
4. Verify environment variable configuration
