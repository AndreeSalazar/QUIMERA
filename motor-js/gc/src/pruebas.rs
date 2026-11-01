//! Pruebas del recolector: listas, ciclos, barrera de escritura y objetos grandes.
use super::*;

fn pequeno() -> Monton {
    Monton::nuevo(Config { guarderia: 256, objeto_grande: 16, umbral_mayor_minimo: 256 })
}

#[test]
fn una_lista_sobrevive_a_muchas_recolecciones() {
    let mut m = pequeno();
    let cabeza = m.raiz(Valor::NULO);
    for n in 0..1000 {
        let siguiente = m.valor(&cabeza);
        let nodo = m.asignar(&[Valor::entero(n), siguiente]);
        m.fijar(&cabeza, nodo);
        for _ in 0..20 {
            m.asignar(&[Valor::entero(-1); 4]); // basura
        }
    }
    let mut v = m.valor(&cabeza);
    for esperado in (0..1000).rev() {
        assert_eq!(m.leer(v, 0), Some(Valor::entero(esperado)));
        v = m.leer(v, 1).unwrap();
    }
    assert_eq!(v, Valor::NULO);
    let e = m.estadisticas();
    assert!(e.menores > 100, "{e:?}");
    assert!(e.mayores > 0, "{e:?}");
    assert!(e.vivos_principal <= 1000 + 64, "la basura se libera: {e:?}");
}

#[test]
fn la_barrera_protege_lo_nuevo_apuntado_desde_lo_viejo() {
    let mut m = pequeno();
    let viejo = m.asignar(&[Valor::NULO]);
    let r = m.raiz(viejo);
    m.menor(); // ahora `viejo` está en la zona principal
    let viejo = m.valor(&r);
    let nuevo = m.asignar(&[Valor::entero(42)]);
    assert!(m.escribir(viejo, 0, nuevo));
    // `nuevo` no está en ninguna raíz: solo lo alcanza la barrera.
    m.menor();
    let hijo = m.leer(m.valor(&r), 0).unwrap();
    assert_eq!(m.leer(hijo, 0), Some(Valor::entero(42)));
}

#[test]
fn los_campos_de_asignar_sobreviven_a_su_propia_recoleccion() {
    let mut m = Monton::nuevo(Config { guarderia: 2, objeto_grande: 16, umbral_mayor_minimo: 1000 });
    let a = m.asignar(&[Valor::entero(7)]);
    m.asignar(&[Valor::NULO]); // guardería llena
    let b = m.asignar(&[a]); // provoca una menor: `a` solo vive en los campos
    let r = m.raiz(b);
    m.menor();
    let a2 = m.leer(m.valor(&r), 0).unwrap();
    assert_eq!(m.leer(a2, 0), Some(Valor::entero(7)));
}

#[test]
fn un_objeto_grande_va_directo_a_la_principal_y_guarda_a_sus_hijos() {
    let mut m = pequeno();
    let hijo = m.asignar(&[Valor::entero(5)]);
    let mut campos = [Valor::NULO; 20];
    campos[3] = hijo;
    let grande = m.asignar(&campos);
    assert_eq!(m.estadisticas().grandes, 1);
    let r = m.raiz(grande);
    m.menor(); // el hijo solo lo alcanza el objeto grande
    let hijo = m.leer(m.valor(&r), 3).unwrap();
    assert_eq!(m.leer(hijo, 0), Some(Valor::entero(5)));
}

#[test]
fn los_ciclos_sin_raiz_se_liberan() {
    let mut m = pequeno();
    let a = m.asignar(&[Valor::NULO]);
    let b = m.asignar(&[a]);
    m.escribir(a, 0, b);
    let ra = m.raiz(a);
    m.mayor();
    assert_eq!(m.estadisticas().vivos_principal, 2);
    m.soltar_raices(ra.0);
    m.mayor();
    assert_eq!(m.estadisticas().vivos_principal, 0, "un contador de referencias no liberaría este ciclo");
}

#[test]
fn las_raices_se_sueltan_como_un_bloque() {
    let mut m = pequeno();
    let marca = m.marca_raices();
    for i in 0..10 {
        let v = m.asignar(&[Valor::entero(i)]);
        m.raiz(v);
    }
    m.mayor();
    assert_eq!(m.estadisticas().vivos_principal, 10);
    m.soltar_raices(marca);
    m.mayor();
    assert_eq!(m.estadisticas().vivos_principal, 0);
}

#[test]
fn una_referencia_invalida_no_rompe_nada() {
    let mut m = pequeno();
    let r = Valor::referencia(Ref::nueva(12345).unwrap());
    assert_eq!(m.leer(r, 0), None);
    assert!(!m.escribir(r, 0, Valor::NULO));
    assert_eq!(m.leer(Valor::entero(3), 0), None);
    let o = m.asignar(&[Valor::NULO]);
    assert_eq!(m.leer(o, 5), None);
    assert!(!m.escribir(o, 5, Valor::NULO));
}

#[test]
fn la_pila_hace_de_registros_y_el_gc_la_actualiza() {
    let mut m = pequeno();
    let base = m.abrir_raices(3, Valor::NULO);
    let o = m.asignar(&[Valor::entero(9)]);
    m.pila_mut()[base + 1] = o;
    m.menor(); // `o` se mueve a la zona principal
    let movido = m.pila()[base + 1];
    assert_ne!(movido, o, "la referencia cambió al moverse");
    assert_eq!(m.leer(movido, 0), Some(Valor::entero(9)));
    m.soltar_raices(base);
    assert_eq!(m.pila().len(), 0);
}

#[test]
fn una_cadena_no_es_un_objeto_del_monton() {
    let mut m = pequeno();
    let o = m.asignar(&[Valor::NULO]);
    let r = m.raiz(o);
    m.mayor(); // `o` queda en el hueco 0 de la zona principal
    // Una cadena con el número 0 no debe mantener vivo ni tocar el objeto 0.
    m.soltar_raices(r.0);
    let marca = m.marca_raices();
    m.raiz(Valor::cadena(Ref::nueva(0).unwrap()));
    m.mayor();
    assert_eq!(m.estadisticas().vivos_principal, 0);
    m.soltar_raices(marca);
}

#[test]
fn la_mayor_anota_las_cadenas_que_alcanza() {
    let mut m = pequeno();
    let cadena = |n: u32| Valor::cadena(Ref::desde_u32(n));
    let obj = m.asignar(&[cadena(3), Valor::entero(1)]);
    let _raiz = m.raiz(obj);
    m.raiz(cadena(70));
    m.asignar(&[cadena(5)]); // basura: su cadena no cuenta
    m.mayor();
    let vistas = m.cadenas_vistas();
    let vista = |n: usize| vistas.get(n / 64).is_some_and(|b| b & (1 << (n % 64)) != 0);
    assert!(vista(3) && vista(70));
    assert!(!vista(5));
}
