# Corpus de pruebas del frontend

Scripts copiados de los benchmarks de Boa (`benches/scripts`, licencia MIT o Unlicense,
https://github.com/boa-dev/boa). Los `v8-benches-*` son de V8 (Octane, vía
`benches/scripts/v8-benches` de Boa) y conservan su cabecera de licencia BSD (Copyright the V8
project authors). El frontend los analiza y compila; la máquina virtual los ejecuta
(`motor-js/vm/tests/ejecutar.rs`) y `bancos/vm` los mide contra Boa. Falta `regexp.js`.
