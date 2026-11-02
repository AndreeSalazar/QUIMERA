export const titulo = (t) => `QUIMERA: ${t}`;
export function lista(xs) { return xs.map((x, i) => `${i + 1}. ${x}`).join('  '); }
