const crypto = require('crypto');

function tokenMatches(expected, provided) {
    if (!expected) return true;
    if (typeof provided !== 'string') return false;
    const left = Buffer.from(expected, 'utf8');
    const right = Buffer.from(provided, 'utf8');
    return left.length === right.length && crypto.timingSafeEqual(left, right);
}

function localApiGuard(expectedToken) {
    return (request, response, next) => {
        if (tokenMatches(expectedToken, request.get('x-nexa-token'))) return next();
        return response.status(401).json({ success: false, error: 'Acesso local do NEXA não autorizado.' });
    };
}

module.exports = { tokenMatches, localApiGuard };
