//! El árbol que construye el analizador, comparado con lo que construyen los navegadores.

use quimera_html::{Dato, analizar};

/// El árbol volcado sin la primera línea de sangría común (más fácil de leer en las pruebas).
fn arbol(html: &str) -> String {
    analizar(html).volcar()
}

fn esperado(s: &str) -> String {
    s.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).map(|l| format!("{}\n", &l[4..])).collect()
}

#[test]
fn crea_html_head_y_body_aunque_falten() {
    assert_eq!(
        arbol("Hola"),
        esperado(
            "
    <html>
      <head>
      <body>
        \"Hola\"
"
        )
    );
    assert_eq!(
        arbol("<!DOCTYPE html><title>T</title><meta charset=utf-8><p>texto"),
        esperado(
            "
    <!DOCTYPE html>
    <html>
      <head>
        <title>
          \"T\"
        <meta charset=\"utf-8\">
      <body>
        <p>
          \"texto\"
"
        )
    );
}

#[test]
fn cierres_implicitos_como_los_navegadores() {
    assert_eq!(
        arbol("<p>uno<p>dos<div>tres</div><ul><li>a<li>b</ul><h1>t<h2>u"),
        esperado(
            "
    <html>
      <head>
      <body>
        <p>
          \"uno\"
        <p>
          \"dos\"
        <div>
          \"tres\"
        <ul>
          <li>
            \"a\"
          <li>
            \"b\"
        <h1>
          \"t\"
        <h2>
          \"u\"
"
        )
    );
}

#[test]
fn tablas_con_tbody_y_tr_implicitos() {
    assert_eq!(
        arbol("<table><td>1<td>2<tr><th>3</table>"),
        esperado(
            "
    <html>
      <head>
      <body>
        <table>
          <tbody>
            <tr>
              <td>
                \"1\"
              <td>
                \"2\"
            <tr>
              <th>
                \"3\"
"
        )
    );
}

#[test]
fn vacios_crudos_comentarios_y_cierres_sueltos() {
    assert_eq!(
        arbol("<div>a<br>b<img src=x.png/>c</span></div><!--c--><script>if(a<b)</script><pre>\nx</pre></p>"),
        esperado(
            "
    <html>
      <head>
      <body>
        <div>
          \"a\"
          <br>
          \"b\"
          <img src=\"x.png/\">
          \"c\"
        <!-- c -->
        <script>
          \"if(a<b)\"
        <pre>
          \"x\"
        <p>
"
        )
    );
}

#[test]
fn utilidades_del_arbol() {
    let doc = analizar("<p class='a b' id=x>Hola <b>mundo</b> &amp; más</p><p>otro");
    let p = doc.buscar("p").unwrap();
    let e = doc.elemento(p).unwrap();
    assert!(e.tiene_clase("b") && !e.tiene_clase("c"));
    assert_eq!(e.atributo("id"), Some("x"));
    assert_eq!(doc.texto(p), "Hola mundo & más");
    let segundo = doc.nodo(doc.nodo(p).padre.unwrap()).hijos[1];
    assert_eq!(doc.elemento_anterior(segundo), Some(p));
    assert!(matches!(doc.nodo(quimera_html::Documento::RAIZ).dato, Dato::Documento));
}

#[test]
fn cualquier_basura_da_un_arbol() {
    for basura in
        ["", "<", "</", "<<<>>>", "<a <b c=", "<!--", "<!DOCTYPE", "</p></p>", "<table><tr></div>", "\u{0}\u{FFFF}<é>"]
    {
        let doc = analizar(basura);
        assert!(doc.buscar("body").is_some(), "{basura:?}");
    }
}
