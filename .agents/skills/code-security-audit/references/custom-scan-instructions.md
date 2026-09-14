# Custom Scan Instructions

Extend the default security audit with organization-specific or domain-specific vulnerability categories. Append these to the vulnerability assessment phase.

## How to Use

Add relevant sections below to the audit based on the project's domain:

## Industry Templates

### Compliance (GDPR, HIPAA, PCI DSS, SOC2)

```
**Compliance-Specific Checks:**
- GDPR Article 17 "Right to Erasure" implementation gaps
- HIPAA PHI encryption at rest violations
- PCI DSS credit card data retention beyond allowed periods
- SOC2 audit trail tampering or deletion capabilities
- CCPA data portability API vulnerabilities
```

### Financial Services

```
**Financial Services Security:**
- Transaction replay attacks in payment processing
- Double-spending vulnerabilities in ledger systems
- Interest calculation manipulation through timing attacks
- Regulatory reporting data tampering
- KYC bypass mechanisms
```

### E-commerce

```
**E-commerce Specific:**
- Shopping cart manipulation for price changes
- Inventory race conditions allowing overselling
- Coupon/discount stacking exploits
- Affiliate tracking manipulation
- Review system authentication bypass
```

### GraphQL APIs

```
**GraphQL Security:**
- Query depth attacks allowing unbounded recursion
- Field-level authorization bypass
- Introspection data leakage in production
- Batch query abuse
```

## Writing Custom Categories

Format:
```
**Category Name:**
- Specific vulnerability or pattern to check
- Clear description of what constitutes this vulnerability
- Why it matters in this context
```

Guidelines:
- Be specific — describe concrete vulnerabilities, not general concerns
- Avoid duplicating default categories (injection, auth, crypto, code execution, data exposure)
- Include context about why something is a vulnerability in your environment
- Same HIGH/MEDIUM severity guidelines apply
