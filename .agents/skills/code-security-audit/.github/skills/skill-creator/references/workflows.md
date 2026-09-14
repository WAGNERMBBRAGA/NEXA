# Workflow Patterns

## Sequential Workflows

For complex tasks, break operations into clear, sequential steps. It is often helpful to give Claude an overview of the process towards the beginning of SKILL.md:

```markdown
Filling a PDF form involves these steps:

1. Analyze the form (run analyze_form.py)
2. Create field mapping (edit fields.json)
3. Validate mapping (run validate_fields.py)
4. Fill the form (run fill_form.py)
5. Verify output (run verify_output.py)
```

## Conditional Workflows

For tasks with branching logic, guide Claude through decision points:

```markdown
1. Determine the modification type:

   **Creating new content?** → Follow "Creation workflow" below
   **Editing existing content?** → Follow "Editing workflow" below

2. Creation workflow:
   [steps]

3. Editing workflow:
   [steps]
```

## Parallel Workflows

When steps can be executed in any order:

```markdown
Complete these validations (any order):

- [ ] Check file format
- [ ] Validate schema
- [ ] Run security scan
- [ ] Test performance

All checks must pass before proceeding.
```

## Error Handling Patterns

Include recovery paths for common failure modes:

```markdown
## Troubleshooting

**If schema validation fails:**
1. Check the error message for specific field issues
2. Run `validate_schema.py --verbose` for detailed output
3. Compare against `references/schema.md` for expected format

**If API returns 401:**
1. Verify credentials in environment variables
2. Check token expiration
3. Re-authenticate using `scripts/refresh_token.py`
```
