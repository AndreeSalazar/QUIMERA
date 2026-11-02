import { titulo, lista } from './lib/formato.js';

const lenguajes = await (await fetch('../datos.json')).json();
document.getElementById('titulo').textContent = titulo('módulos funcionando');
const p = document.getElementById('detalle');
p.className = 'ok';
p.textContent = lista(lenguajes.lenguajes);
