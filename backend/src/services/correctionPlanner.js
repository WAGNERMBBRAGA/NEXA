function correctionEvidence(inspection = {}, audit = {}) {
    return {
        findings: (inspection.findings || []).filter(finding => ['error', 'warning'].includes(finding.severity)),
        checks: (audit.checks || []).filter(check => !check.ok && check.name !== 'docker_disponivel')
    };
}

module.exports = { correctionEvidence };
