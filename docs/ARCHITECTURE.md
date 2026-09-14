# Arquitetura do Sistema - Nova Liguação de Programação NEXA

## Visão Geral

Sistema completo para gestão de vendas, estoque e financeiro com interface web moderna em React e API RESTful em Node.js/Express.

---

## Stack Tecnológico

### Frontend
- **React 18** - Framework UI principal
- **TypeScript** - Tipagem estática
- **Tailwind CSS** - Estilização utility-first
- **React Router v6** - Roteamento
- **Vite** - Build tool e desenvolvimento

### Backend
- **Node.js** - Runtime JavaScript
- **Express.js** - Framework web
- **Prisma ORM** - Database abstraction
- **PostgreSQL** - Banco de dados relacional

---

## Estrutura do Projeto

```
nexa/
├── frontend/           # Aplicação React
│   ├── src/
│   │   ├── components/  # Components reutilizáveis
│   │   ├── pages/       # Páginas principais
│   │   ├── services/    # API calls e lógica de negócio
│   │   ├── hooks/       # Custom hooks
│   │   └── App.tsx      # Componente raiz
│   └── package.json
├── backend/            # API Express
│   ├── src/
│   │   ├── controllers/  # Lógica de negócio
│   │   ├── routes/       # Endpoints HTTP
│   │   ├── models/       # Prisma schemas
│   │   └── utils/        # Utilitários
│   └── package.json
├── docker-compose.yml  # Containerização
└── README.md
```

---

## Módulos Principais

### 1. **Vendas** (`/api/vendas`)
- Criação e edição de vendas
- Listagem histórica de vendas
- Relatório financeiro por período
- Integração com estoque

### 2. **Estoque** (`/api/estoque`)
- Cadastro de produtos/serviços
- Controle de quantidades
- Rastreamento de movimentações
- Alertas de reabastecimento

### 3. **Financeiro** (`/api/financeiro`)
- Registro de receitas e despesas
- Balancete contábil
- Extratos por período
- Relatórios de fluxo de caixa

---

## Fluxo de Dados

```
User → React App (Frontend)
        ↓ HTTP REST API
    Express Server (Backend)
        ↓ Prisma ORM
   PostgreSQL Database
```

### Endpoints Principais

| Método | Endpoint | Descrição |
|--------|----------|-----------|
| GET    | `/api/vendas` | Listar vendas |
| POST   | `/api/vendas` | Criar venda |
| PUT    | `/api/vendas/:id` | Atualizar venda |
| DELETE | `/api/vendas/:id` | Excluir venda |
| GET    | `/api/estoque` | Listar produtos |
| POST   | `/api/estoque` | Cadastrar produto |

---

## Modelos de Dados Principais

### User
- `id`: UUID (PK)
- `email`: string
- `name`: string
- `password_hash`: string

### Product
- `id`: UUID (PK)
- `name`: string
- `description?:` text
- `price`: decimal(10,2)
- `quantity`: integer
- `restock_threshold`: integer

### Sale
- `id`: UUID (PK)
- `products[]`: Product[] (many-to-many)
- `total_amount`: decimal(10,2)
- `sale_date`: timestamp
- `seller_id`: User ID (FK)

---

## Tecnologias de Infraestrutura

| Serviço | Tecnologia | Propósito |
|---------|------------|-----------|
| API Gateway | Nginx | Reverse proxy e load balancing |
| Frontend Hosting | Docker + Nginx | Serve builds do React |
| Backend | Express (Docker) | Processa requisições HTTP |
| Database | PostgreSQL (Docker) | Persistência de dados |

---

## Segurança

### Autenticação
- JWT (JSON Web Tokens)
- Refresh tokens para sessões longas
- Validacao em todos os endpoints

### Proteção de Dados
- Senhas sempre hashadas com bcrypt
- SQL injection prevenido via Prisma ORM
- CORS configurado por domínio permitido

---

## Notas Importantes

⚠️ **Em desenvolvimento** - Algumas funcionalidades ainda estão sendo implementadas.
