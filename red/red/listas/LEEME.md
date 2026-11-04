# Listas de bloqueo de terceros

Estas listas van tal cual se publican, incluidas en el binario de QUIMERA (`quimera-red`, ver [docs/RED.md](../../../docs/RED.md#bloqueo-de-anuncios-y-rastreo)). No se descargan al navegar.

| Archivo | Qué es | De dónde | Versión |
|---|---|---|---|
| `easylist.txt` | EasyList: anuncios | <https://easylist.to/easylist/easylist.txt> | 202611040458 (4 nov 2026) |
| `easyprivacy.txt` | EasyPrivacy: rastreo | <https://easylist.to/easylist/easyprivacy.txt> | 202611040451 (4 nov 2026) |

**Licencia.** Las escribe el equipo de EasyList (<https://easylist.to/>) y se publican con licencia **GPLv3 o Creative Commons BY-SA 3.0**, a elegir (<https://easylist.to/pages/licence.html>). Esa licencia es la de estos dos archivos, no la del resto de QUIMERA (MIT o Apache-2.0). Si se reparten cambiados, tiene que ser con la misma licencia y diciendo de dónde vienen.

**Actualizar.** Basta con bajar los dos archivos de nuevo y sustituirlos aquí: el filtro los lee al arrancar. Después, `cargo test -p quimera-red` comprueba que la lista de serie sigue bloqueando lo de siempre.
