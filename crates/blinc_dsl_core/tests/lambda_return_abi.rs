//! Raw-TypedAST isolation: does a JIT'd zero-arg lambda return its value?
//! No Blinc grammar, no Blinc passes — the AST is built by hand here.
use zyntax_embed::ZyntaxRuntime;
use zyntax_typed_ast::type_registry::{
    AsyncKind, CallingConvention, NullabilityKind, PrimitiveType, Type, Visibility,
};
use zyntax_typed_ast::typed_ast::{
    TypedBlock, TypedDeclaration, TypedFunction, TypedLambda, TypedLambdaBody, TypedLiteral,
};
use zyntax_typed_ast::{Span, TypedExpression, TypedNode, TypedProgram, TypedStatement};

fn n<T>(node: T, ty: Type) -> TypedNode<T> {
    TypedNode::new(node, ty, Span::default())
}
fn f64_ty() -> Type {
    Type::Primitive(PrimitiveType::F64)
}
fn fn_ty_ret_f64() -> Type {
    Type::Function {
        params: vec![],
        return_type: Box::new(f64_ty()),
        is_varargs: false,
        has_named_params: false,
        has_default_params: false,
        async_kind: AsyncKind::Sync,
        calling_convention: CallingConvention::Default,
        nullability: NullabilityKind::NonNull,
    }
}
fn func(name: &str, ret: Type, body: TypedBlock) -> TypedNode<TypedDeclaration> {
    #[allow(clippy::needless_update)]
    let f = TypedFunction {
        name: zyntax_typed_ast::InternedString::new_global(name),
        return_type: ret,
        body: Some(body),
        visibility: Visibility::Public,
        ..Default::default()
    };
    n(
        TypedDeclaration::Function(f),
        Type::Primitive(PrimitiveType::Unit),
    )
}

#[test]
fn raw_ast_lambda_return() {
    let mut rt = ZyntaxRuntime::new().expect("runtime");
    let mut prog = TypedProgram::default();

    // A) plain fn: `fn plain_f64() -> f64 { return 0.25 }`
    prog.declarations.push(func(
        "plain_f64",
        f64_ty(),
        TypedBlock {
            statements: vec![n(
                TypedStatement::Return(Some(Box::new(n(
                    TypedExpression::Literal(TypedLiteral::Float(0.25)),
                    f64_ty(),
                )))),
                f64_ty(),
            )],
            span: Span::default(),
        },
    ));

    // B) fn returning a zero-arg lambda whose body is `0.25`,
    //    annotated `Type::Function{ -> f64 }` (the shape Blinc needs).
    let lambda = n(
        TypedExpression::Lambda(TypedLambda {
            params: vec![],
            captures: vec![],
            body: TypedLambdaBody::Block(TypedBlock {
                statements: vec![n(
                    TypedStatement::Expression(Box::new(n(
                        TypedExpression::Literal(TypedLiteral::Float(0.25)),
                        f64_ty(),
                    ))),
                    Type::Primitive(PrimitiveType::Unit),
                )],
                span: Span::default(),
            }),
        }),
        fn_ty_ret_f64(),
    );
    prog.declarations.push(func(
        "make_lambda",
        Type::Primitive(PrimitiveType::I64),
        TypedBlock {
            statements: vec![n(
                TypedStatement::Return(Some(Box::new(lambda))),
                Type::Primitive(PrimitiveType::I64),
            )],
            span: Span::default(),
        },
    ));

    match rt.compile_typed_program(prog) {
        Ok(_) => println!("compile: OK"),
        Err(e) => {
            println!("compile: FAILED {e}");
            return;
        }
    }

    // A: direct f64 return from a JIT'd fn
    match rt.get_function_ptr("plain_f64") {
        Some(p) => {
            let f: extern "C" fn() -> f64 = unsafe { std::mem::transmute(p) };
            println!("A plain fn -> f64      = {}   (want 0.25)", f());
        }
        None => println!("A plain_f64: no ptr"),
    }

    // B: lambda pointer, then call it as fn() -> f64 (Blinc's exact FFI shape)
    match rt.get_function_ptr("make_lambda") {
        Some(p) => {
            let mk: extern "C" fn() -> i64 = unsafe { std::mem::transmute(p) };
            let closure = mk();
            println!("B make_lambda -> ptr   = {closure:#x}");
            if closure != 0 {
                let g: extern "C" fn() -> f64 = unsafe { std::mem::transmute(closure) };
                println!("B lambda() -> f64      = {}   (want 0.25)", g());
            }
        }
        None => println!("B make_lambda: no ptr"),
    }
}
