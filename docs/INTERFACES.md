# Interface Pública da API - Nova Liguação de Programação NEXA

## Base URL

```
http://localhost:3001/api
```

---

## Autenticação

### Header Required

```
Authorization: Bearer <JWT_TOKEN>
Content-Type: application/json
```

---

## Endpoints de Vendas (`/api/vendas`)

### GET /vendas

Retorna lista de vendas paginadas.

**Query Parameters:**
| Parâmetro | Tipo | Descrição |
|-----------|------|-----------|
| page | integer (default: 1) | Página a ser retornada |
| limit | integer (default: 20) | Quantidade de itens por página |
| startDate | string (ISO date) | Filtro por data inicial |
| endDate | string (ISO date) | Filtro por data final |

**Response:**
```json
{
  "data": [/* array of sale objects */],
  "meta": {
    "page": 1,
    "limit": 20,
    "total": 150
  }
}
```

### POST /vendas

Cria nova venda.

**Request Body:**
```json
{
  "products": [/* array of product IDs */],
  "subtotal": 150.00,
  "discount?: 10.00"
}
```

**Response:** `201 Created`
```json
{
  "id": "uuid-here",
  "status": "completed",
  "total_amount": 140.00,
  "created_at": "2026-09-02T10:30:00Z"
}
```

### PUT /vendas/:id

Atualiza venda existente.

**Request Body:**
```json
{
  "status": "completed",
  "discount": 15.00,
  "notes?: "Cliente preferencial"
}
```

### DELETE /vendas/:id

Exclui venda (soft delete).

---

## Endpoints de Estoque (`/api/estoque`)

### GET /estoque

Retorna lista de produtos paginada.

**Query Parameters:**
| Parâmetro | Tipo | Descrição |
|-----------|------|-----------|
| page | integer (default: 1) | Página a ser retornada |
| limit | integer (default: 20) | Quantidade de itens por página |
| search | string | Busca parcial no nome |

**Response:**
```json
{
  "data": [/* array of product objects */],
  "meta": {
    "page": 1,
    "limit": 20,
    "total": 45
  }
}
```

### POST /estoque

Cadastra novo produto.

**Request Body:**
```json
{
  "name": "Exemplo Produto",
  "description?: "Descrição detalhada",
  "price": 199.90,
  "quantity": 50,
  "restock_threshold": 10
}
```

**Response:** `201 Created`
```json
{
  "id": "uuid-here",
  "name": "Exemplo Produto",
  "price": 199.90,
  "quantity": 50,
  "in_stock": true
}
```

### PUT /estoque/:id

Atualiza produto.

**Request Body:**
```json
{
  "name": "Novo Nome",
  "price": 179.90,
  "quantity": 75,
  "restock_threshold?: 20
}
```

### POST /estoque/:id/restock

Avisa reposição de estoque.

**Request Body:**
```json
{
  "quantity_added": 30,
  "reason?: "Compra externa"
}
```

---

## Erros da API

| Código | Descrição |
|--------|-----------|
| `400` | Bad Request - Dados inválidos |
| `401` | Unauthorized - Token inválido ou expirado |
| `403` | Forbidden - Permissão negada |
| `404` | Not Found - Recurso não encontrado |
| `500` | Internal Server Error |

**Response padrão de erro:**
```json
{
  "success": false,
  "error": {
    "message": "Descrição do erro",
    "code": "VALIDATION_ERROR"
  }
}
```

---

## Exemplo de Uso (cURL)

### Criar Venda
```bash
curl -X POST http://localhost:3001/api/vendas \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "products": ["uuid-1", "uuid-2"],
    "subtotal": 150.00,
    "discount": 10.00
  }'
```

### Listar Produtos
```bash
curl -X GET http://localhost:3001/api/estoque?page=1&limit=10 \
  -H "Authorization: Bearer YOUR_TOKEN"
```

---

## Notas Importantes

⚠️ **Versão Atual**: v1.0.0-beta  
📝 Última atualização: 2026-09-02
