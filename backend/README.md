# NEXA Backend Server

## Setup Instructions

1. **Install dependencies:**
```bash
npm install
```

2. **Configure Ollama (optional):**
   - Start Ollama: `ollama serve`
   - Pull models: `ollama pull llama3.2`

3. **Start the server:**
```bash
npm start
```

The API will be available at `http://localhost:3001`

## Available Endpoints

- `/api/chat/status` - Check AI service status and available providers
- `/api/chat/models` - Get list of available models for selector dropdown  
- `/api/chat` (POST) - Send prompt and get response from AI
