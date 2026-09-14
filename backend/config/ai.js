/**
 * AI Configuration for NEXA Project
 */

module.exports = {
    ollama: {
        enabled: true, // Set to false if you prefer API-based models only
        host: 'http://localhost:11434',
        modelsToLoad: [
            'llama3.2'
        ]
    },
    
    openai: {
        enabled: false, // Default is false (uses Ollama if available)
        apiKey: null
    }
};
