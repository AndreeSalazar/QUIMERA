var div = document.createElement('div'), inp = document.createElement('input'), t = document.createTextNode('x');
console.log(div instanceof HTMLDivElement, div instanceof HTMLElement, div instanceof Element, div instanceof Node, div instanceof EventTarget, inp instanceof HTMLInputElement, inp instanceof HTMLDivElement, t instanceof Text, t instanceof Element, document instanceof Document, Node.ELEMENT_NODE, Object.prototype.toString.call(div));
class Saludo extends HTMLElement {
  static get observedAttributes() { return ['nombre']; }
  constructor() { super(); this.veces = 0; }
  connectedCallback() { this.textContent = 'Hola ' + (this.getAttribute('nombre') || 'mundo'); console.log('conectado', this.veces); }
  disconnectedCallback() { console.log('desconectado'); }
  attributeChangedCallback(n, a, b) { this.veces++; console.log('atributo', n, a, b); }
}
customElements.define('saludo-x', Saludo);
var nuevo = document.createElement('saludo-x');
nuevo.setAttribute('nombre', 'luis');
document.body.appendChild(nuevo);
var directo = new Saludo();
console.log(nuevo instanceof Saludo, nuevo instanceof HTMLElement, directo.tagName, directo.veces, customElements.get('saludo-x') === Saludo, document.body.innerHTML);
nuevo.remove();
var tpl = document.createElement('template');
tpl.innerHTML = '<b>t</b><i>u</i>';
console.log(tpl.content.childNodes.length, tpl.content.firstChild.tagName, tpl.childNodes.length, tpl.content.cloneNode(true).childNodes.length);
var visto = '';
document.body.addEventListener('hola', function (e) { visto = e.detail + ' ' + (e.target === div) + ' ' + (e instanceof CustomEvent); });
document.body.appendChild(div);
console.log(div.dispatchEvent(new CustomEvent('hola', { detail: 5, bubbles: true })), visto);
var c = new Event('c', { cancelable: true });
div.addEventListener('c', function (e) { e.preventDefault(); });
console.log(div.dispatchEvent(c), c.defaultPrevented, new Event('x').bubbles);
div.insertAdjacentHTML('beforeend', '<span>a</span>');
div.prepend('antes');
div.append('después');
console.log(div.innerHTML, div.compareDocumentPosition(document.body), document.body.compareDocumentPosition(div));
customElements.whenDefined('otro-x').then(function () { console.log('definido después'); });
customElements.define('otro-x', class extends HTMLElement {});
