# Custom API Base URLs

## Supported Endpoints

| Provider | Base URL | Notes |
|----------|----------|-------|
| OpenAI | https://api.openai.com/v1 | Default |
| Ollama | http://localhost:11434/v1 | Local |
| LM Studio | http://localhost:1234/v1 | Local |
| vLLM | http://localhost:8000/v1 | Local |
| Together AI | https://api.together.xyz/v1 | Cloud |
| Groq | https://api.groq.com/openai/v1 | Cloud |

## Setup

### UI Configuration

1. Open Settings > AI Provider
2. Select "Custom" as provider
3. Enter Base URL (e.g., http://localhost:11434/v1)
4. Enter API Key (use "ollama" for Ollama)
5. Select Model (e.g., llama3)
6. Click Test Connection

### Environment Variables

```bash
export SOTTO_API_BASE_URL=http://localhost:11434/v1
export SOTTO_API_KEY=ollama
export SOTTO_MODEL=llama3
```

## Architecture

All requests are normalized to OpenAI chat completions format, ensuring compatibility with any OpenAI-compatible endpoint.

## Troubleshooting

- Connection refused: Ensure LLM server is running
- Model not found: Check with `ollama list`
- Timeout: Increase timeout for slower models
- CORS: Configure server to allow Sotto requests
