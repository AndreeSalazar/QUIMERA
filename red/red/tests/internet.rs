//! Contra internet de verdad. No corren en el hook (dependen de la red); a mano:
//! `cargo test -p quimera-red --test internet -- --ignored`

use quimera_red::{Cliente, Error, Url};

fn pedir(u: &str) -> Result<quimera_red::Respuesta, Error> {
    Cliente::nuevo().pedir(&Url::analizar(u).unwrap())
}

#[test]
#[ignore = "necesita internet"]
fn https_valido_y_redireccion_de_http_a_https() {
    let r = pedir("https://example.com/").unwrap();
    assert!(r.exito() && r.texto().contains("<title>Example Domain</title>"), "{}", r.estado);
    let r = pedir("http://github.com/").unwrap();
    assert_eq!(r.url.esquema, "https", "github.com manda de http a https");
}

#[test]
#[ignore = "necesita internet"]
fn certificados_no_validos_se_rechazan() {
    for (url, motivo) in [
        ("https://expired.badssl.com/", "Expired"),
        ("https://wrong.host.badssl.com/", "NotValidForName"),
        ("https://self-signed.badssl.com/", "UnknownIssuer"),
        ("https://untrusted-root.badssl.com/", "UnknownIssuer"),
    ] {
        match pedir(url) {
            Err(Error::Tls(m)) => assert!(m.contains(motivo), "{url}: {m}"),
            otro => panic!("{url}: se esperaba un error de TLS y hubo {otro:?}"),
        }
    }
}
