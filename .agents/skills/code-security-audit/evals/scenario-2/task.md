# Task: Set Up GitHub Action Security Audit for a PR

A team wants to add automated security review to their CI pipeline. They use GitHub Actions and have an Anthropic API key stored as a repository secret `CLAUDE_API_KEY`.

Requirements:
1. Create the GitHub Actions workflow YAML that triggers on pull requests
2. Include proper permissions for PR commenting and content reading
3. Show the correct environment variables needed to run the audit script
4. Explain how findings get posted as PR review comments
5. Show how to configure excluded directories (they want to exclude `tests/` and `docs/`)

Provide the complete setup instructions including the workflow file and any additional configuration needed.
