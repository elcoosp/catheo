//! Lexical approximations over opaque closure bodies (§3.2, §3.4).
//!
//! Both checks are *syntax-level and conservative*: they may produce false
//! positives (rejecting a fusion that would have been safe); false negatives
//! are unacceptable and must never happen.

use proc_macro2::{TokenStream, TokenTree};

/// Marker idents that reveal container-shape observation (§3.2).
const OBSERVER_IDENTS: [&str; 4] = ["Some", "None", "Ok", "Err"];

/// Method idents that reveal container-shape observation when they directly
/// follow a `.` punct.
const OBSERVER_METHODS: [&str; 3] = ["len", "is_empty", "iter"];

/// Conservative lexical approximation of "this closure observes the
/// container's shape" (§3.2).
///
/// Returns `true` when the body's token text contains `.len`, `.is_empty`,
/// `.iter`, `Some`, `None`, `Ok`, or `Err` at any nesting depth.
#[must_use]
pub fn is_structural_observer(body: &TokenStream) -> bool {
    scan_observer(body, false)
}

fn scan_observer(tokens: &TokenStream, after_dot: bool) -> bool {
    let mut after_dot = after_dot;
    for tt in tokens.clone() {
        match tt {
            TokenTree::Ident(ref ident) => {
                let name = ident.to_string();
                if OBSERVER_IDENTS.contains(&name.as_str())
                    || (after_dot && OBSERVER_METHODS.contains(&name.as_str()))
                {
                    return true;
                }
                after_dot = false;
            }
            TokenTree::Punct(ref punct) => {
                after_dot = punct.as_char() == '.';
            }
            TokenTree::Group(ref group) => {
                if scan_observer(&group.stream(), after_dot) {
                    return true;
                }
            }
            TokenTree::Literal(_) => {
                after_dot = false;
            }
        }
    }
    false
}

/// Lexical purity check for `fold` closures (§3.4).
///
/// Returns `false` when the body contains `&mut` anywhere; `true` otherwise.
#[must_use]
pub fn is_lexically_pure(body: &TokenStream) -> bool {
    !scan_mut_ref(body, false)
}

fn scan_mut_ref(tokens: &TokenStream, after_amp: bool) -> bool {
    let mut after_amp = after_amp;
    for tt in tokens.clone() {
        match tt {
            TokenTree::Ident(ref ident) => {
                if after_amp && ident == "mut" {
                    return true;
                }
                after_amp = false;
            }
            TokenTree::Punct(ref punct) => {
                after_amp = punct.as_char() == '&';
            }
            TokenTree::Group(ref group) => {
                if scan_mut_ref(&group.stream(), after_amp) {
                    return true;
                }
            }
            TokenTree::Literal(_) => {
                after_amp = false;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{is_lexically_pure, is_structural_observer};
    use quote::quote;

    #[test]
    fn observer_scan_handles_all_markers() {
        assert!(is_structural_observer(&quote!(x.len())));
        assert!(is_structural_observer(&quote!(x.is_empty())));
        assert!(is_structural_observer(&quote!(x.iter())));
        assert!(is_structural_observer(&quote!(Some(x))));
        assert!(is_structural_observer(&quote!(None)));
        assert!(is_structural_observer(&quote!(Ok(x))));
        assert!(is_structural_observer(&quote!(Err(e))));
        assert!(!is_structural_observer(&quote!(parse_tokens(&x))));
        assert!(!is_structural_observer(&quote!(x * 2)));
    }

    #[test]
    fn observer_scan_recurses_into_groups() {
        assert!(is_structural_observer(&quote!({ deep.len() })));
        assert!(is_structural_observer(&quote!(foo(Ok(1)))));
        assert!(!is_structural_observer(&quote!(foo(bar(x)))));
    }

    #[test]
    fn len_without_dot_is_not_an_observer() {
        assert!(!is_structural_observer(&quote!(let len = x;)));
        assert!(is_structural_observer(&quote!(x.len)));
    }

    #[test]
    fn purity_scan_detects_mut_ref() {
        assert!(is_lexically_pure(&quote!(acc + x)));
        assert!(!is_lexically_pure(&quote!(&mut acc)));
        assert!(!is_lexically_pure(&quote!({ push(&mut acc, x) })));
        assert!(is_lexically_pure(&quote!(x & mask)));
        assert!(is_lexically_pure(&quote!(mut x = 3)));
    }
}
