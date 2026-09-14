const fs = require('fs');
const path = require('path');

const text = lines => lines.join('\n') + '\n';

function restaurantFiles() {
    const system = text([
        "'use strict';",
        'function createSystem() {',
        ' const restaurants=new Map(),tables=new Map(),products=new Map(),tabs=new Map(),offline=[];',
        " const id=p=>p+'_'+Date.now().toString(36)+Math.random().toString(36).slice(2,6);",
        " const get=(map,key,label)=>{const value=map.get(key);if(!value)throw new Error(label+' não encontrado');return value};",
        ' return {',
        "  restaurant:d=>{const x={id:id('rest'),name:String(d.name||'').trim(),createdAt:new Date().toISOString()};if(!x.name)throw new Error('Nome obrigatório');restaurants.set(x.id,x);return x},",
        "  table:d=>{get(restaurants,d.restaurantId,'Restaurante');const x={id:id('table'),restaurantId:d.restaurantId,label:String(d.label||''),status:'free'};if(!x.label)throw new Error('Mesa obrigatória');tables.set(x.id,x);return x},",
        "  product:d=>{get(restaurants,d.restaurantId,'Restaurante');const x={id:id('prod'),restaurantId:d.restaurantId,name:String(d.name||''),price:Number(d.price||0),stock:Number(d.stock||0)};if(!x.name||x.price<0||x.stock<0)throw new Error('Produto inválido');products.set(x.id,x);return x},",
        "  tab:d=>{get(restaurants,d.restaurantId,'Restaurante');if(d.tableId)get(tables,d.tableId,'Mesa').status='occupied';const x={id:id('tab'),restaurantId:d.restaurantId,tableId:d.tableId||null,channel:d.channel||'counter',status:'open',items:[],payments:[]};tabs.set(x.id,x);return x},",
        "  item:(tabId,d)=>{const tab=get(tabs,tabId,'Comanda'),p=get(products,d.productId,'Produto'),q=Math.max(1,Number(d.quantity||1));if(p.stock<q)throw new Error('Estoque insuficiente');p.stock-=q;const x={id:id('item'),productId:p.id,name:p.name,quantity:q,unitPrice:p.price,status:'kitchen_pending'};tab.items.push(x);return x},",
        "  payment:(tabId,d)=>{const tab=get(tabs,tabId,'Comanda'),x={id:id('pay'),method:d.method||'cash',amount:Number(d.amount||0)};if(x.amount<=0)throw new Error('Pagamento inválido');tab.payments.push(x);const total=tab.items.reduce((s,i)=>s+i.quantity*i.unitPrice,0),paid=tab.payments.reduce((s,i)=>s+i.amount,0);if(paid>=total){tab.status='closed';if(tab.tableId)get(tables,tab.tableId,'Mesa').status='free'}return {payment:x,total,paid,status:tab.status}},",
        "  offline:d=>{const x={id:id('offline'),operation:d,queuedAt:new Date().toISOString()};offline.push(x);return x},",
        "  dashboard:restId=>({restaurant:get(restaurants,restId,'Restaurante'),tables:[...tables.values()].filter(x=>x.restaurantId===restId),products:[...products.values()].filter(x=>x.restaurantId===restId),tabs:[...tabs.values()].filter(x=>x.restaurantId===restId),offline:[...offline]})",
        ' };',
        '}',
        'module.exports={createSystem};'
    ]);
    const server = text([
        "'use strict';",
        "const http=require('node:http');const fs=require('node:fs');const path=require('node:path');const {createSystem}=require('./system');",
        "const send=(res,status,body,type='application/json; charset=utf-8')=>{res.writeHead(status,{'content-type':type,'access-control-allow-origin':'*'});res.end(body)};",
        "const json=(res,status,body)=>send(res,status,JSON.stringify(body));",
        "const body=req=>new Promise((ok,fail)=>{let raw='';req.on('data',x=>raw+=x);req.on('end',()=>{try{ok(raw?JSON.parse(raw):{})}catch{fail(new Error('JSON inválido'))}})});",
        "function createApp(system=createSystem()){return http.createServer(async(req,res)=>{if(req.method==='GET'&&req.url==='/')return send(res,200,fs.readFileSync(path.join(__dirname,'../../web/index.html'),'utf8'),'text/html; charset=utf-8');if(req.method==='GET'&&req.url==='/health')return json(res,200,{ok:true,service:'restaurant-saas'});try{const d=await body(req);if(req.method==='POST'&&req.url==='/api/restaurants')return json(res,201,{data:system.restaurant(d)});if(req.method==='POST'&&req.url==='/api/tables')return json(res,201,{data:system.table(d)});if(req.method==='POST'&&req.url==='/api/products')return json(res,201,{data:system.product(d)});if(req.method==='POST'&&req.url==='/api/tabs')return json(res,201,{data:system.tab(d)});let m=req.url.match(/^\\/api\\/tabs\\/([^/]+)\\/items$/);if(req.method==='POST'&&m)return json(res,201,{data:system.item(m[1],d)});m=req.url.match(/^\\/api\\/tabs\\/([^/]+)\\/payments$/);if(req.method==='POST'&&m)return json(res,201,{data:system.payment(m[1],d)});if(req.method==='POST'&&req.url==='/api/offline-queue')return json(res,201,{data:system.offline(d)});m=req.url.match(/^\\/api\\/restaurants\\/([^/]+)\\/dashboard$/);if(req.method==='GET'&&m)return json(res,200,{data:system.dashboard(m[1])});return json(res,404,{error:'Rota não encontrada'})}catch(e){return json(res,400,{error:e.message})}})}",
        "if(require.main===module)createApp().listen(Number(process.env.PORT||3000),()=>console.log('Restaurant SaaS: http://localhost:3000'));",
        'module.exports={createApp};'
    ]);
    const entry = text(["'use strict';", "const {createApp}=require('./restaurant/server');", "if(require.main===module)createApp().listen(Number(process.env.PORT||3000),()=>console.log('Restaurant SaaS: http://localhost:3000'));", 'module.exports={createApp};']);
    const web = `<!doctype html><html lang="pt-BR"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Restaurant SaaS</title><style>*{box-sizing:border-box}body{margin:0;background:#f6f7fb;color:#172033;font:15px system-ui}.top{background:#14213d;color:white;padding:22px max(24px,5vw);display:flex;justify-content:space-between;align-items:center}.top h1{margin:0;font-size:22px}.top span{color:#b7c6e6}.grid{padding:26px max(24px,5vw);display:grid;grid-template-columns:1fr 1fr;gap:18px}.card{background:white;border:1px solid #e1e5ee;border-radius:14px;padding:18px;box-shadow:0 4px 16px #14213d0b}.card h2{font-size:16px;margin:0 0 12px}input,button{padding:10px;border-radius:8px;border:1px solid #cbd3e1;font:inherit}input{width:100%;margin:5px 0}button{cursor:pointer;background:#3257d6;color:#fff;border:0;font-weight:700;margin-top:6px}.muted{color:#65708a}.status{margin:16px max(24px,5vw);padding:12px;border-radius:9px;background:#edf2ff;color:#2442a0}@media(max-width:700px){.grid{grid-template-columns:1fr}}</style><header class="top"><div><h1>Restaurant SaaS</h1><span>Mesas, comandas, cozinha, caixa e modo offline</span></div><b id="rest">Não configurado</b></header><p id="status" class="status">Crie o restaurante para iniciar a operação.</p><main class="grid"><section class="card"><h2>1. Restaurante e mesa</h2><input id="name" placeholder="Nome do restaurante"><button onclick="createRestaurant()">Criar restaurante</button><input id="table" placeholder="Mesa (ex.: 12)"><button onclick="createTable()">Adicionar mesa</button></section><section class="card"><h2>2. Cardápio</h2><input id="product" placeholder="Produto"><input id="price" type="number" placeholder="Preço"><input id="stock" type="number" placeholder="Estoque"><button onclick="createProduct()">Adicionar produto</button></section><section class="card"><h2>3. Comandas</h2><button onclick="openTab()">Abrir comanda de balcão</button><p class="muted">Os pedidos recebem estado <b>kitchen_pending</b> para a cozinha.</p></section><section class="card"><h2>Painel operacional</h2><pre id="dash" class="muted">Aguardando restaurante…</pre></section></main><script>let restaurant,table,product,tab;const $=id=>document.getElementById(id);async function api(url,data){const r=await fetch(url,{method:data?'POST':'GET',headers:data?{'content-type':'application/json'}:{},body:data?JSON.stringify(data):undefined});const x=await r.json();if(!r.ok)throw Error(x.error);return x.data}function note(x){$('status').textContent=x}async function createRestaurant(){try{restaurant=await api('/api/restaurants',{name:$('name').value});$('rest').textContent=restaurant.name;note('Restaurante criado. Cadastre mesas e produtos.');dash()}catch(e){note(e.message)}}async function createTable(){try{table=await api('/api/tables',{restaurantId:restaurant.id,label:$('table').value});note('Mesa '+table.label+' adicionada.');dash()}catch(e){note(e.message)}}async function createProduct(){try{product=await api('/api/products',{restaurantId:restaurant.id,name:$('product').value,price:$('price').value,stock:$('stock').value});note('Produto '+product.name+' adicionado.');dash()}catch(e){note(e.message)}}async function openTab(){try{tab=await api('/api/tabs',{restaurantId:restaurant.id,tableId:table&&table.id,channel:'waiter'});note('Comanda aberta para atendimento.');dash()}catch(e){note(e.message)}}async function dash(){if(!restaurant)return;const x=await api('/api/restaurants/'+restaurant.id+'/dashboard');$('dash').textContent=JSON.stringify({mesas:x.tables.length,produtos:x.products.length,comandas:x.tabs.length,offline:x.offline.length},null,2)}</script></html>\n`;
    return { system, server, entry, web };
}

function upsert(projectRoot, file, content) {
    return fs.existsSync(path.join(projectRoot, ...file.split('/')))
        ? [{ kind: 'read_file', path: file }, { kind: 'write_file', path: file, content }]
        : [{ kind: 'create_file', path: file, content }];
}

function restaurantBootstrapActions(projectRoot) {
    if (!projectRoot) return [];
    const files = restaurantFiles();
    if (fs.existsSync(path.join(projectRoot, 'src', 'restaurant', 'server.js')) && fs.existsSync(path.join(projectRoot, 'web', 'index.html'))) return [];
    return [
        ...upsert(projectRoot, 'src/restaurant/system.js', files.system),
        ...upsert(projectRoot, 'src/restaurant/server.js', files.server),
        ...upsert(projectRoot, 'src/server.js', files.entry),
        ...upsert(projectRoot, 'web/index.html', files.web),
        ...upsert(projectRoot, 'README.md', '# Restaurant SaaS\n\nBase funcional: restaurantes, mesas, comandas, produtos, estoque, pedidos e caixa.\n\nExecute `npm start` e abra `http://localhost:3000`.\n'),
        { kind: 'run_command', command: ['node', '--check', 'src/server.js'] },
        { kind: 'run_command', command: ['node', '--check', 'src/restaurant/server.js'] }
    ];
}

module.exports = { restaurantBootstrapActions };
