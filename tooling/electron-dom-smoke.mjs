const port = Number(process.argv[2]);
if (!Number.isInteger(port)) throw new Error('Informe a porta de depuração do Electron.');

const deadline = Date.now() + 15000;
let page;
while (Date.now() < deadline) {
    try {
        const pages = await fetch(`http://127.0.0.1:${port}/json`).then(response => response.json());
        page = pages.find(item => item.type === 'page' && item.title === 'NEXA AI' && item.url.startsWith('http://127.0.0.1:'));
        if (page) break;
    } catch {}
    await new Promise(resolve => setTimeout(resolve, 250));
}
if (!page) throw new Error('A janela do NEXA não apareceu no tempo esperado.');

const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve, { once: true });
    socket.addEventListener('error', reject, { once: true });
});

const snapshotExpression = `JSON.stringify({
    title: document.title,
    ready: document.readyState,
    sidebar: !!document.querySelector('.sidebar'),
    composer: !!document.querySelector('#chat-form textarea'),
    conversations: !!document.querySelector('#conv-list'),
    projectPanel: !!document.querySelector('#project-section'),
    modelStore: !!document.querySelector('#model-store-modal'),
    onboardingPresent: !!document.querySelector('#model-onboarding'),
    onboardingVisible: !document.querySelector('#model-onboarding')?.classList.contains('hidden'),
    fatalText: /Cannot GET|Uncaught Exception/i.test(document.body.innerText)
})`;
const expression = `new Promise(resolve => {
    const deadline = Date.now() + 12000;
    const inspect = () => {
        if (document.querySelector('.sidebar') || Date.now() >= deadline) return resolve(${snapshotExpression});
        setTimeout(inspect, 200);
    };
    inspect();
})`;

const result = await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error('Tempo esgotado ao inspecionar a janela.')), 15000);
    socket.addEventListener('message', event => {
        const message = JSON.parse(event.data);
        if (message.id !== 1) return;
        clearTimeout(timeout);
        if (!message.result?.result) return reject(new Error(`Falha no protocolo de depuração: ${JSON.stringify(message)}`));
        resolve(message.result.result.value);
    });
    socket.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression, returnByValue: true, awaitPromise: true } }));
});
console.log(result);
if (process.argv.includes('--close')) {
    socket.send(JSON.stringify({ id: 2, method: 'Browser.close' }));
    await new Promise(resolve => setTimeout(resolve, 750));
}
socket.close();
