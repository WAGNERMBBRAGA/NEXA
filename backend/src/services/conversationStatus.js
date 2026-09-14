function isVerifiedStatusQuestion(message) {
    return /\b(?:voce|você)\s+(?:corrigiu|alterou|fez)|\bfoi\s+corrigid[oa]\b|\bquais\s+(?:alteracoes|alterações)\b|\bo\s+que\s+(?:voce|você)\s+fez\b/i.test(String(message || ''));
}

function verifiedChangeSummary(actionResults) {
    const changes = (actionResults || []).filter(result => result.ok && ['write_file', 'create_file', 'replace_text', 'create_project', 'document_development_modes'].includes(result.kind));
    if (!changes.length) return '';
    const labels = { create_file: 'arquivo criado', write_file: 'arquivo atualizado', replace_text: 'trecho corrigido', create_project: 'projeto criado', document_development_modes: 'ambientes documentados' };
    return 'Sim. Esta conversa registrou alterações verificáveis:\n\n' + changes.map(result => `- ${labels[result.kind] || result.kind}: ${(result.details || result).path || 'concluído'}`).join('\n') + '\n\nA etapa foi revalidada depois das alterações.';
}

module.exports = { isVerifiedStatusQuestion, verifiedChangeSummary };
