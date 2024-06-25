use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::Read;
use std::process;
use syn::parse_file;
use syn::visit::Visit;

fn main() {
    let mut args = env::args();
    let _ = args.next(); // executable name

    let filename = match (args.next(), args.next()) {
        (Some(filename), None) => filename,
        _ => {
            eprintln!("Usage: dump-syntax path/to/filename.rs");
            process::exit(1);
        }
    };

    let mut file = File::open(&filename).expect("unable to open file");

    let mut src = String::new();
    file.read_to_string(&mut src).expect("unable to read file");

    let mut syntax = parse_file(&src).expect("unable to parse file");

    let overflows = oracle(&syntax);

    repair(&mut syntax, &mut overflows);
}

struct ArrayDeclVisitor<'ast> {
    current_id: String,
    current_size: Option<usize>,
    arrays: HashMap<String, Option<(usize, &'ast syn::Lit)>>,
}

impl<'ast> Visit<'ast> for ArrayDeclVisitor<'ast> {
    fn visit_local(&mut self, node: &'ast syn::Local) {
        self.visit_pat(&node.pat);
    }

    fn visit_pat(&mut self, node: &'ast syn::Pat) {
        match node {
            syn::Pat::Type(t) => self.visit_pat_type(&t),
            syn::Pat::Ident(i) => self.visit_pat_ident(&i),
            _ => (),
        }
    }

    fn visit_pat_type(&mut self, node: &'ast syn::PatType) {
        match &*node.ty {
            syn::Type::Array(a) => {
                self.visit_pat(&*node.pat);
                self.visit_type_array(&a);
            }
            syn::Type::Reference(r) => {
                self.visit_pat(&*node.pat);
                self.visit_type_reference(&r)
            }
            syn::Type::Slice(s) => {
                self.visit_pat(&*node.pat);
                self.visit_type_slice(&s)
            }
            _ => (),
        }
    }

    fn visit_pat_ident(&mut self, node: &'ast syn::PatIdent) {
        self.current_id = node.ident.to_string();
    }

    fn visit_type_array(&mut self, node: &'ast syn::TypeArray) {
        if let syn::Expr::Lit(l) = &node.len {
            if let syn::Lit::Int(size) = &l.lit {
                self.current_size = size.base10_parse::<usize>().ok();
                self.arrays.insert(
                    self.current_id.clone(),
                    Some((size.base10_parse::<usize>().unwrap(), &l.lit)),
                );
            }
        }
    }

    fn visit_type_reference(&mut self, node: &'ast syn::TypeReference) {
        self.visit_type(&node.elem);
    }

    fn visit_type_slice(&mut self, _node: &'ast syn::TypeSlice) {
        self.current_size = None;
    }
}

#[derive(Debug)]
struct SmallArrayOverflowInfo<'a> {
    transfer_size: usize,
    buffer_decl: &'a syn::Lit,
}

#[derive(Debug)]
struct RiskOverflowInfo<'a> {
    unsafe_block: usize,
    block: &'a syn::Block,
    stmt_id: usize,
}

#[derive(Debug)]
enum OverflowType<'a> {
    Risk(RiskOverflowInfo<'a>),
    SmallArray(SmallArrayOverflowInfo<'a>),
}

#[derive(Debug)]
struct OverflowDetectVisitor<'ast> {
    arrays: HashMap<String, Option<(usize, &'ast syn::Lit)>>,
    size: Option<usize>,
    dst_id: String,
    overflows_info: Vec<OverflowType<'ast>>,
}

impl<'ast> Visit<'ast> for OverflowDetectVisitor<'_> {
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*node.func {
            let mut func_path = String::new();
            for seg in p.path.segments.iter() {
                func_path.push_str("::");
                func_path.push_str(&seg.ident.to_string());
            }

            if func_path.contains("::ptr::copy") {
                let mut transfer_size: Option<usize> = None;
                let mut dst_size: Option<usize> = None;
                let mut dst_id: Option<String> = None;
                // Get the size of the destination
                if let syn::Expr::MethodCall(dst) = &node.args[1] {
                    self.visit_expr_method_call(&dst);
                    dst_size = self.size;
                    dst_id = Some(self.dst_id.clone());
                }
                // Get the size of the transfer
                if let syn::Expr::MethodCall(count) = &node.args[2] {
                    self.visit_expr_method_call(&count);
                    transfer_size = self.size;
                }
                if let syn::Expr::Lit(count) = &node.args[2] {
                    self.visit_expr_lit(&count);
                    transfer_size = self.size;
                }

                // Say if there is indeed a buffer overflow or if it depends of runtime
                println!("{:?} {:?}", dst_size, transfer_size);
                if dst_size.is_none() || transfer_size.is_none() {
                    println!("Risk of buffer overflow at runtime");
                } else if let (Some(d), Some(t)) = (dst_size, transfer_size) {
                    self.overflows_info
                        .push(OverflowType::SmallArray(SmallArrayOverflowInfo {
                            transfer_size: transfer_size.unwrap(),
                            buffer_decl: self.arrays.get(&dst_id.unwrap()).unwrap().unwrap().1,
                        }));
                    if t > d {
                    } else {
                        println!("No risk of buffer overflow detected");
                    }
                }
            }
        }
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if let syn::Expr::Path(p) = &*node.receiver {
            self.dst_id = p.path.segments[0].ident.to_string();
            self.size = self
                .arrays
                .get(&self.dst_id)
                .unwrap_or(&None)
                .and_then(|x| Some(x.0.clone()));
        }
    }

    fn visit_expr_lit(&mut self, node: &'ast syn::ExprLit) {
        if let syn::Lit::Int(count) = &node.lit {
            self.size = count.base10_parse::<usize>().ok().clone();
        }
    }
}

fn oracle<'ast>(ast: &'ast syn::File) -> Vec<OverflowType<'ast>> {
    let mut array_visitor = ArrayDeclVisitor {
        current_id: String::new(),
        current_size: None,
        arrays: HashMap::new(),
    };
    array_visitor.visit_file(ast);
    //println!("{:#?}", array_visitor.arrays);

    let mut overflow_detect_visitor = OverflowDetectVisitor {
        arrays: array_visitor.arrays,
        size: None,
        dst_id: String::new(),
        overflows_info: Vec::new(),
    };
    overflow_detect_visitor.visit_file(ast);
    println!("{:#?}", overflow_detect_visitor.overflows_info);
    overflow_detect_visitor.overflows_info
}

fn repair(ast: &mut syn::File, overflows: &mut [OverflowType]) {
    // repair the program
    for of in overflows {
        match of {
            OverflowType::SmallArray(sa) => {
                if let syn::Lit::Int(_) = sa.buffer_decl {
                    sa.buffer_decl = &syn::Lit::Int(syn::LitInt::new(
                        &sa.transfer_size.to_string(),
                        sa.buffer_decl.span(),
                    ));
                }
            }
            _ => (),
        }
    }
}
