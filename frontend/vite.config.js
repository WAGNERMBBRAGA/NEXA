import { defineConfig } from 'vite';

// Configuração do Vite para o frontend do NEXA.
// O proxy encaminha as chamadas /api para o backend Express (porta 3001),
// resolvendo o ERR_CONNECTION_REFUSED que ocorria porque o frontend
// chamava o backend diretamente sem passar pelo proxy.
export default defineConfig({
    server: {
        host: '0.0.0.0',
        port: 5173,
        proxy: {
            '/api': {
                target: process.env.NEXA_BACKEND_URL || 'http://localhost:3001',
                changeOrigin: true
            }
        }
    }
});
