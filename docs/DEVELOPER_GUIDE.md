# Guia de Desenvolvimento - Nova Liguação de Programação NEXA

## Pré-requisitos

- **Node.js** v18+ (LTS)
- **Docker** e **Docker Compose**
- **Git**
- **Pnpm** ou **npm**

---

## Estrutura do Projeto

```bash
nexa/
├── frontend/        # Aplicação React
│   ├── package.json
│   └── vite.config.ts
├── backend/         # API Express
│   ├── package.json
│   └── prisma/
│       └── schema.prisma
└── docker-compose.yml
```

---

## Inicialização Local (Desenvolvimento)

### Opção 1: Docker Compose (Recomendado)

```bash
# Clone o repositório
git clone <repository-url> nexa

# Entre no diretório
cd nexa

# Inicie os serviços
docker-compose up -d

# Acesse frontend (porta 5173)
http://localhost:5173

# Acesse backend API (porta 3001)
http://localhost:3001/api/vendas
```

### Opção 2: Desenvolvimento Manual

#### Backend (Node.js/Express)

```bash
cd backend
npm install

# Iniciar servidor de desenvolvimento
npm run dev

# Ou com Prisma migrations
npx prisma migrate dev
npx prisma generate
node server.js
```

#### Frontend (React + Vite)

```bash
cd frontend
npm install

# Desenvolvimento com hot-reload
npm run dev

# Build para produção
npm run build
```

---

## Database Setup

### Migrations

```bash
# Gerar novas migrations
npx prisma migrate dev --name add_new_field

# Reversar migration
npx prisma migrate revert

# Gerar tipos TypeScript
npx prisma generate

# Reset database (com cuidado!)
npx prisma migrate reset
```

---

## Development Workflow

### 1. Criando Novo Endpoint

**Backend:**

```bash
cd backend

# Adicionar novo arquivo de controller
touch src/controllers/novo-endpoint.controller.js

# Registrar endpoint em routes
src/routes/novo-endpoint.route.js:
import { controller } from './novo-endpoint.controller';
export const router = express.Router((req, res, next) => {
  // ... configuration
});
```

**Frontend:**

- Adicionar nova página em `frontend/src/pages/`
- Criar API service em `frontend/src/services/`
- Link com React Router

---

### 2. Criando Novo Modelo (Prisma)

```prisma
// prisma/schema.prisma
model NovoModelo {
  id        String   @id @default(auto()) @map("@id") @db.Uuid
  nome      String
  descricao? String
  
  @@map("novos_modelos")
}
```

Gerar migrations e tipos:
```bash
npx prisma migrate dev --name add_novo_modelo
npx prisma generate
```

---

### 3. Debugging

**Backend (Node.js):**

- Inspeção de heap: `--inspect=9222`
- Logs: `DEBUG=* express:*`

**Frontend:**

- DevTools do Chrome/Firefox
- React Developer Tools
- Vite built-in hot reload

---

## Deploy Production

### Docker (Recomendado)

```bash
# Build e subir
docker-compose up -d --build

# Verificar logs
docker-compose logs -f backend
docker-compose logs -f frontend
```

### Frontend Build

```bash
cd frontend
npm run build
# Arquivos estão em dist/ para deploy
```

---

## Conventions & Style Guides

### Backend (Node.js)

- **NOMES DE ARQUIVOS**: kebab-case (`user-controller.js`)
- **NOMES DE VARIÁVEIS**: camelCase (`userName`, `isReady`)
- **TYPESCRIPT**: Optional com `?` (`name?: string`)

### Frontend (React)

- **COMPONENTS**: PascalCase (`UserProfile`, `Button`)
- **VARIÁVEIS E CONSTANTES**: camelCase (`userProfile`, `MAX_RETRIES: 5`)
- **HOOKS**: prefixado com `use` (`useDataFetcher`)

---

## Troubleshooting

| Problema | Solução |
|----------|---------|
| Port already in use | Change port numbers in docker-compose.yml |
| Prisma not generating | Run `npx prisma generate` again |
| Frontend build failing | Check console for Vite errors |
| Database connection failed | Verify PostgreSQL is running and accessible |

---

## Recursos Úteis

- [React Documentation](https://react.dev)
- [Express.js Docs](https://expressjs.com/)
- [Prisma Guide](https://www.prisma.io/docs)
- [Docker Compose Reference](https://docs.docker.com/compose/reference/)

---

## Contribuição

Para contribuir com o projeto:

1. Fork o repositório
2. Crie uma branch (`git checkout -b feature/nome`)
3. Faça as alterações
4. Teste localmente
5. Envie um Pull Request
