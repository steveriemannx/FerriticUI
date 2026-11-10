//! The `ui!` macro: turns a small DSL into a **flat static tree**.
//!
//! ```ignore
//! ui! {
//!     msg = Msg;
//!     struct CounterUi {
//!         Column(padding: 16.0, spacing: 10.0, align: Align::Center) {
//!             value: Label(text: "0"),
//!             buttons: Row(spacing: 10.0) {
//!                 inc: Button<Msg>(text: "+", on_click: Msg::Increment),
//!                 reset: Button<Msg>(text: "Reset", on_click: Msg::Reset),
//!             }
//!         }
//!     }
//! }
//! ```
//!
//! The generated struct has one field per node (named nodes are `pub`), plus a
//! `[NodeMeta; N]` array. The node graph is generated code: a private
//! `enum Node`, per-container `const` child lists, and one `match` per pass
//! (measure / arrange / paint / hit / event / focus / theme), so there is no
//! runtime tree and no dynamic dispatch.
//!
//! Node types must implement the `ferritic_core` traits:
//! * leaves: `Construct`, `WidgetView`, and `MessageSource<Msg>` (non-interactive
//!   widgets can rely on the blanket `MessageSource` impl of `Label`-like
//!   widgets only if they implement it themselves),
//! * containers: `Construct` + `ContainerLayout`.
//!
//! Properties are assigned as struct fields on the constructed widget:
//! `prop: value` becomes `widget.prop = value.into()`, and `on_*: value`
//! becomes `widget.on_* = Some(value)`. Bare float literals get an `f32` suffix
//! (all numeric widget properties are `f32`), which avoids the compiler's
//! float-literal fallback lint on newer toolchains.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    braced,
    parse::{Parse, ParseStream},
    parse_macro_input,
    parenthesized,
    punctuated::Punctuated,
    token, Expr, Ident, Token, Type, Visibility,
};

/// Converts a bare float literal (or its negation) into an `f32` literal.
///
/// Widget properties are `f32`; relying on `.into()` for `24.0` lets the
/// compiler infer `f64` first and then "fall back" to `f32`, which newer
/// toolchains warn about (and will eventually reject).
fn coerce_prop_value(value: &Expr) -> TokenStream2 {
    fn float_tokens(float: &syn::LitFloat) -> TokenStream2 {
        let text = format!("{}f32", float.base10_digits());
        text.parse().expect("a valid f32 literal")
    }
    match value {
        Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Float(float),
            ..
        }) => float_tokens(float),
        Expr::Unary(syn::ExprUnary {
            op: syn::UnOp::Neg(_),
            expr,
            ..
        }) => match &**expr {
            Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Float(float),
                ..
            }) => {
                let tokens = float_tokens(float);
                quote! { -#tokens }
            }
            _ => quote! { (#value).into() },
        },
        _ => quote! { (#value).into() },
    }
}

struct UiInput {
    msg: Type,
    vis: Visibility,
    name: Ident,
    root: NodeAst,
}

struct NodeAst {
    name: Option<Ident>,
    ty: Type,
    props: Vec<(Ident, Expr)>,
    children: Vec<NodeAst>,
}

struct Prop {
    name: Ident,
    value: Expr,
}

impl Parse for Prop {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let value: Expr = input.parse()?;
        Ok(Self { name, value })
    }
}

impl Parse for NodeAst {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = parse_optional_name(input)?;
        let ty: Type = input.parse()?;

        let props = if input.peek(token::Paren) {
            let content;
            parenthesized!(content in input);
            let list = Punctuated::<Prop, Token![,]>::parse_terminated(&content)?;
            list.into_iter().map(|prop| (prop.name, prop.value)).collect()
        } else {
            Vec::new()
        };

        let children = if input.peek(token::Brace) {
            let content;
            braced!(content in input);
            let mut children = Vec::new();
            while !content.is_empty() {
                children.push(content.parse::<NodeAst>()?);
                if content.peek(Token![,]) {
                    content.parse::<Token![,]>()?;
                } else if !content.is_empty() {
                    return Err(content.error("expected `,` or `}`"));
                }
            }
            children
        } else {
            Vec::new()
        };

        Ok(Self {
            name,
            ty,
            props,
            children,
        })
    }
}

impl Parse for UiInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let keyword: Ident = input.parse()?;
        if keyword != "msg" {
            return Err(syn::Error::new(
                keyword.span(),
                "expected `msg = MessageType;` first",
            ));
        }
        input.parse::<Token![=]>()?;
        let msg: Type = input.parse()?;
        input.parse::<Token![;]>()?;

        let vis: Visibility = input.parse()?;
        input.parse::<Token![struct]>()?;
        let name: Ident = input.parse()?;

        let content;
        braced!(content in input);
        let root: NodeAst = content.parse()?;
        if !content.is_empty() {
            return Err(content.error("unexpected tokens after the root node"));
        }

        Ok(Self {
            msg,
            vis,
            name,
            root,
        })
    }
}

fn parse_optional_name(input: ParseStream) -> syn::Result<Option<Ident>> {
    if input.peek(Ident) {
        let fork = input.fork();
        let _ident: Ident = fork.parse()?;
        if fork.peek(Token![::]) {
            return Ok(None);
        }
        if fork.peek(Token![:]) {
            let name: Ident = input.parse()?;
            input.parse::<Token![:]>()?;
            return Ok(Some(name));
        }
    }
    Ok(None)
}

struct FlatNode {
    ty: Type,
    name: Option<Ident>,
    props: Vec<(Ident, Expr)>,
    children: Vec<usize>,
}

fn flatten(node: NodeAst, out: &mut Vec<FlatNode>) -> usize {
    let index = out.len();
    out.push(FlatNode {
        ty: node.ty,
        name: node.name,
        props: node.props,
        children: Vec::new(),
    });
    let mut child_ids = Vec::with_capacity(node.children.len());
    for child in node.children {
        child_ids.push(flatten(child, out));
    }
    out[index].children = child_ids;
    index
}

#[proc_macro]
pub fn ui(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as UiInput);
    expand(input).into()
}

fn expand(input: UiInput) -> TokenStream2 {
    let UiInput {
        msg,
        vis,
        name,
        root,
    } = input;

    let mut nodes = Vec::new();
    flatten(root, &mut nodes);

    let count = nodes.len();
    let node_enum = format_ident!("{}Node", name);
    let variants: Vec<Ident> = (0..count).map(|index| format_ident!("N{}", index)).collect();
    let fields: Vec<Ident> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| match &node.name {
            Some(name) => name.clone(),
            None if index == 0 => format_ident!("__root"),
            None => format_ident!("__node_{}", index),
        })
        .collect();
    let types: Vec<&Type> = nodes.iter().map(|node| &node.ty).collect();
    let root_variant = &variants[0];

    let construct = nodes.iter().enumerate().map(|(index, node)| {
        let variable = format_ident!("__n{}", index);
        let ty = &node.ty;
        let props = node.props.iter().map(|(prop, value)| {
            if prop.to_string().starts_with("on_") {
                quote! { #variable.#prop = ::core::option::Option::Some(#value); }
            } else {
                let value = coerce_prop_value(value);
                quote! { #variable.#prop = #value; }
            }
        });
        quote! {
            let mut #variable = <#ty as ::ferritic_core::Construct>::construct(theme);
            #(#props)*
        }
    });

    let construct_stmts = quote! { #(#construct)* };

    let field_inits = nodes.iter().enumerate().map(|(index, _)| {
        let field = &fields[index];
        let variable = format_ident!("__n{}", index);
        quote! { #field: #variable }
    });
    let field_inits_tokens = quote! { #(#field_inits,)* };

    let children_consts = nodes.iter().enumerate().filter_map(|(index, node)| {
        if node.children.is_empty() {
            return None;
        }
        let const_name = format_ident!("CHILDREN_{}", index);
        let child_variants: Vec<&Ident> = node.children.iter().map(|child| &variants[*child]).collect();
        Some(quote! {
            const #const_name: &'static [#node_enum] = &[#(#node_enum::#child_variants),*];
        })
    });

    let children_consts_tokens = quote! { #(#children_consts)* };

    let all_nodes = {
        let variants = &variants;
        quote! {
            const ALL_NODES: &'static [#node_enum] = &[#(#node_enum::#variants),*];
        }
    };

    let measure_arms = nodes.iter().enumerate().map(|(index, node)| {
        let variant = &variants[index];
        let field = &fields[index];
        let ty = &node.ty;
        if node.children.is_empty() {
            quote! {
                #node_enum::#variant => ::ferritic_core::WidgetView::measure(
                    &mut self.#field,
                    &mut ::ferritic_core::MeasureCx { text: &mut *text },
                    constraints,
                ),
            }
        } else {
            let const_name = format_ident!("CHILDREN_{}", index);
            let child_count = node.children.len();
            quote! {
                #node_enum::#variant => {
                    let child_constraints = <#ty as ::ferritic_core::ContainerLayout>::child_constraints(
                        &self.#field,
                        constraints,
                    );
                    let mut sizes = [::ferritic_core::Size::ZERO; #child_count];
                    for (index, child) in Self::#const_name.iter().enumerate() {
                        sizes[index] = self.measure_node(&mut *text, *child, child_constraints);
                    }
                    <#ty as ::ferritic_core::ContainerLayout>::measure_children(
                        &mut self.#field,
                        constraints,
                        &sizes,
                    )
                }
            }
        }
    });

    let arrange_arms = nodes.iter().enumerate().map(|(index, node)| {
        let variant = &variants[index];
        let field = &fields[index];
        let ty = &node.ty;
        if node.children.is_empty() {
            quote! { #node_enum::#variant => {} }
        } else {
            let const_name = format_ident!("CHILDREN_{}", index);
            let child_count = node.children.len();
            quote! {
                #node_enum::#variant => {
                    let sizes = self.child_sizes(Self::#const_name);
                    let mut rects = [::ferritic_core::Rect::ZERO; #child_count];
                    <#ty as ::ferritic_core::ContainerLayout>::arrange_children(
                        &mut self.#field,
                        rect,
                        &sizes,
                        &mut rects,
                    );
                    for (index, child) in Self::#const_name.iter().enumerate() {
                        self.arrange_node(*child, rects[index]);
                    }
                }
            }
        }
    });

    let paint_arms = nodes.iter().enumerate().map(|(index, node)| {
        let variant = &variants[index];
        let field = &fields[index];
        let ty = &node.ty;
        if node.children.is_empty() {
            quote! {
                #node_enum::#variant => {
                    ::ferritic_core::WidgetView::paint(&mut self.#field, cx, rect);
                }
            }
        } else {
            let const_name = format_ident!("CHILDREN_{}", index);
            quote! {
                #node_enum::#variant => {
                    let old_clip = cx.clip;
                    if let Some(clip) = <#ty as ::ferritic_core::ContainerLayout>::child_clip(
                        &self.#field,
                        rect,
                    ) {
                        cx.set_clip(Some(clip));
                    }
                    ::ferritic_core::ContainerLayout::paint_background(&self.#field, cx, rect);
                    for child in Self::#const_name.iter() {
                        self.paint_node(cx, *child);
                    }
                    cx.clip = old_clip;
                    ::ferritic_core::ContainerLayout::paint_foreground(&self.#field, cx, rect);
                }
            }
        }
    });

    let hit_arms = nodes.iter().enumerate().map(|(index, node)| {
        let variant = &variants[index];
        let field = &fields[index];
        if node.children.is_empty() {
            quote! {
                #node_enum::#variant => {
                    if ::ferritic_core::WidgetView::hit(&self.#field, point, meta.rect) {
                        Some(node)
                    } else {
                        None
                    }
                }
            }
        } else {
            let const_name = format_ident!("CHILDREN_{}", index);
            quote! {
                #node_enum::#variant => {
                    for child in Self::#const_name.iter().rev() {
                        if let Some(hit) = self.hit_test(*child, point) {
                            return Some(hit);
                        }
                    }
                    None
                }
            }
        }
    });

    let dispatch_arms = nodes.iter().enumerate().filter_map(|(index, node)| {
        if !node.children.is_empty() {
            return None;
        }
        let variant = &variants[index];
        let field = &fields[index];
        Some(quote! {
            #node_enum::#variant => ::ferritic_core::MessageSource::emit(
                &mut self.#field,
                event,
                cx,
                rect,
                text,
            ),
        })
    });

    let theme_arms = nodes.iter().enumerate().filter_map(|(index, node)| {
        if !node.children.is_empty() {
            return None;
        }
        let variant = &variants[index];
        let field = &fields[index];
        Some(quote! {
            #node_enum::#variant => {
                ::ferritic_core::WidgetView::apply_theme(&mut self.#field, theme);
            }
        })
    });

    let focus_arms = nodes.iter().enumerate().map(|(index, node)| {
        let variant = &variants[index];
        let field = &fields[index];
        if node.children.is_empty() {
            quote! { #node_enum::#variant => ::ferritic_core::WidgetView::focusable(&self.#field), }
        } else {
            quote! { #node_enum::#variant => false, }
        }
    });

    let from_u32_arms = variants.iter().enumerate().map(|(index, variant)| {
        let index = index as u32;
        quote! { #index => ::core::option::Option::Some(#node_enum::#variant), }
    });
    let from_u32_body = quote! { #(#from_u32_arms)* };

    let scroll_arms = nodes.iter().enumerate().map(|(index, node)| {
        let variant = &variants[index];
        let field = &fields[index];
        let ty = &node.ty;
        if node.children.is_empty() {
            quote! { #node_enum::#variant => false, }
        } else {
            let const_name = format_ident!("CHILDREN_{}", index);
            quote! {
                #node_enum::#variant => {
                    for child in Self::#const_name.iter().rev() {
                        if self.scroll_node(*child, point, delta, cx) {
                            return true;
                        }
                    }
                    if self.meta[node as usize].rect.contains(point) {
                        <#ty as ::ferritic_core::ContainerLayout>::wheel(&mut self.#field, delta, cx)
                    } else {
                        false
                    }
                }
            }
        }
    });
    let scroll_arms_tokens = quote! { #(#scroll_arms)* };

    let measure_arms_tokens = quote! { #(#measure_arms)* };
    let arrange_arms_tokens = quote! { #(#arrange_arms)* };
    let paint_arms_tokens = quote! { #(#paint_arms)* };
    let hit_arms_tokens = quote! { #(#hit_arms)* };
    let dispatch_arms_tokens = quote! { #(#dispatch_arms)* };
    let theme_arms_tokens = quote! { #(#theme_arms)* };
    let focus_arms_tokens = quote! { #(#focus_arms)* };

    let field_decls = quote! { #(pub #fields: #types,)* };
    let enum_body = {
        let variants = &variants;
        quote! { #(#variants,)* }
    };

    quote! {
        #vis struct #name {
            #field_decls
            meta: [::ferritic_core::NodeMeta; #count],
        }

        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(usize)]
        enum #node_enum {
            #enum_body
        }

        impl #name {
            pub fn new(theme: &::ferritic_core::Theme) -> Self {
                #construct_stmts
                Self {
                    #field_inits_tokens
                    meta: [::ferritic_core::NodeMeta::ZERO; #count],
                }
            }

            #children_consts_tokens
            #all_nodes

            fn node_from_u32(value: u32) -> ::core::option::Option<#node_enum> {
                match value {
                    #from_u32_body
                    _ => ::core::option::Option::None,
                }
            }

            fn child_sizes(&self, children: &'static [#node_enum]) -> ::std::vec::Vec<::ferritic_core::Size> {
                children
                    .iter()
                    .map(|child| self.meta[*child as usize].measured)
                    .collect()
            }

            fn measure_node(
                &mut self,
                text: &mut dyn ::ferritic_core::TextEngine,
                node: #node_enum,
                constraints: ::ferritic_core::Constraints,
            ) -> ::ferritic_core::Size {
                let size = match node {
                    #measure_arms_tokens
                };
                self.meta[node as usize].measured = size;
                size
            }

            fn arrange_node(&mut self, node: #node_enum, rect: ::ferritic_core::Rect) {
                self.meta[node as usize].rect = rect;
                match node {
                    #arrange_arms_tokens
                }
            }

            fn paint_node(&mut self, cx: &mut ::ferritic_core::PaintCx<'_>, node: #node_enum) {
                let rect = self.meta[node as usize].rect;
                match node {
                    #paint_arms_tokens
                }
            }

            fn hit_test(
                &self,
                node: #node_enum,
                point: ::ferritic_core::Point,
            ) -> ::core::option::Option<#node_enum> {
                let meta = self.meta[node as usize];
                if !meta.rect.contains(point) {
                    return ::core::option::Option::None;
                }
                match node {
                    #hit_arms_tokens
                }
            }

            fn dispatch_node(
                &mut self,
                node: #node_enum,
                event: &::ferritic_core::Event,
                cx: &mut ::ferritic_core::EventCtx,
                text: &mut dyn ::ferritic_core::TextEngine,
            ) -> ::core::option::Option<#msg> {
                let rect = self.meta[node as usize].rect;
                match node {
                    #dispatch_arms_tokens
                    _ => ::core::option::Option::None,
                }
            }

            fn apply_theme_node(&mut self, theme: &::ferritic_core::Theme) {
                for node in Self::ALL_NODES.iter() {
                    match *node {
                        #theme_arms_tokens
                        _ => {}
                    }
                }
            }

            fn change_focus(
                &mut self,
                cx: &mut ::ferritic_core::EventCtx,
                target: ::core::option::Option<u32>,
                text: &mut dyn ::ferritic_core::TextEngine,
            ) {
                if cx.focus == target {
                    return;
                }
                if let Some(previous) = cx.focus.and_then(Self::node_from_u32) {
                    self.dispatch_node(
                        previous,
                        &::ferritic_core::Event::FocusLost,
                        cx,
                        text,
                    );
                }
                cx.focus = target;
                if let Some(next) = target.and_then(Self::node_from_u32) {
                    self.dispatch_node(next, &::ferritic_core::Event::FocusGained, cx, text);
                }
            }

            fn scroll_node(
                &mut self,
                node: #node_enum,
                point: ::ferritic_core::Point,
                delta: f32,
                cx: &mut ::ferritic_core::EventCtx,
            ) -> bool {
                match node {
                    #scroll_arms_tokens
                }
            }

            fn is_focusable(&self, node: #node_enum) -> bool {
                match node {
                    #focus_arms_tokens
                }
            }
        }

        impl ::ferritic_core::StaticUi for #name {
            type Msg = #msg;

            fn apply_theme(&mut self, theme: &::ferritic_core::Theme) {
                self.apply_theme_node(theme);
            }

            fn measure(
                &mut self,
                text: &mut dyn ::ferritic_core::TextEngine,
                constraints: ::ferritic_core::Constraints,
            ) -> ::ferritic_core::Size {
                self.measure_node(text, #node_enum::#root_variant, constraints)
            }

            fn arrange(&mut self, rect: ::ferritic_core::Rect) {
                self.arrange_node(#node_enum::#root_variant, rect);
            }

            fn paint(
                &mut self,
                text: &mut dyn ::ferritic_core::TextEngine,
                scene: &mut ::ferritic_core::Scene,
            ) {
                let mut cx = ::ferritic_core::PaintCx {
                    scene,
                    text,
                    clip: ::core::option::Option::None,
                };
                self.paint_node(&mut cx, #node_enum::#root_variant);
            }

            fn event(
                &mut self,
                cx: &mut ::ferritic_core::EventCtx,
                event: &::ferritic_core::Event,
                text: &mut dyn ::ferritic_core::TextEngine,
            ) -> ::core::option::Option<#msg> {
                match event {
                    ::ferritic_core::Event::PointerMoved(position) => {
                        let hit = self.hit_test(#node_enum::#root_variant, *position);
                        if hit.map(|node| node as u32) != cx.hovered {
                            if let Some(previous) = cx.hovered.and_then(Self::node_from_u32) {
                                self.dispatch_node(
                                    previous,
                                    &::ferritic_core::Event::PointerLeft,
                                    cx,
                                    text,
                                );
                            }
                            cx.hovered = hit.map(|node| node as u32);
                        }
                        if let Some(node) = hit {
                            self.dispatch_node(node, event, cx, text)
                        } else {
                            ::core::option::Option::None
                        }
                    }
                    ::ferritic_core::Event::PointerLeft => {
                        if let Some(previous) = cx.hovered.take().and_then(Self::node_from_u32) {
                            self.dispatch_node(previous, event, cx, text);
                        }
                        ::core::option::Option::None
                    }
                    ::ferritic_core::Event::PointerDown(position, _) => {
                        let hit = self.hit_test(#node_enum::#root_variant, *position);
                        let focus = hit.filter(|node| self.is_focusable(*node));
                        self.change_focus(cx, focus.map(|node| node as u32), text);
                        cx.pressed = hit.map(|node| node as u32);
                        if let Some(node) = hit {
                            self.dispatch_node(node, event, cx, text)
                        } else {
                            ::core::option::Option::None
                        }
                    }
                    ::ferritic_core::Event::PointerUp(position, _) => {
                        let target = cx
                            .pressed
                            .take()
                            .and_then(Self::node_from_u32)
                            .or_else(|| self.hit_test(#node_enum::#root_variant, *position));
                        if let Some(node) = target {
                            self.dispatch_node(node, event, cx, text)
                        } else {
                            ::core::option::Option::None
                        }
                    }
                    ::ferritic_core::Event::Key {
                        key: ::ferritic_core::Key::Tab,
                        pressed: true,
                        shift,
                        ..
                    } => {
                        self.focus_next(cx, !shift, text);
                        cx.request_redraw();
                        ::core::option::Option::None
                    }
                    ::ferritic_core::Event::Key { .. } => {
                        if let Some(node) = cx.focus.and_then(Self::node_from_u32) {
                            self.dispatch_node(node, event, cx, text)
                        } else {
                            ::core::option::Option::None
                        }
                    }
                    ::ferritic_core::Event::FocusLost => {
                        if let Some(node) = cx.focus.take().and_then(Self::node_from_u32) {
                            self.dispatch_node(node, event, cx, text);
                        }
                        cx.ime_allowed = false;
                        cx.ime_cursor = ::core::option::Option::None;
                        ::core::option::Option::None
                    }
                    ::ferritic_core::Event::FocusGained => {
                        if let Some(node) = cx.focus.and_then(Self::node_from_u32) {
                            self.dispatch_node(node, event, cx, text)
                        } else {
                            ::core::option::Option::None
                        }
                    }
                    ::ferritic_core::Event::ImeCommit(_)
                    | ::ferritic_core::Event::ImePreedit(_)
                    | ::ferritic_core::Event::ImeDisabled => {
                        if let Some(node) = cx.focus.and_then(Self::node_from_u32) {
                            self.dispatch_node(node, event, cx, text)
                        } else {
                            ::core::option::Option::None
                        }
                    }
                    ::ferritic_core::Event::Wheel(delta) => {
                        cx.wheel_consumed = false;
                        let hit = self.hit_test(#node_enum::#root_variant, cx.pointer);
                        if let Some(node) = hit {
                            self.dispatch_node(node, event, cx, text);
                        }
                        if !cx.wheel_consumed {
                            self.scroll_node(
                                #node_enum::#root_variant,
                                cx.pointer,
                                *delta,
                                cx,
                            );
                        }
                        ::core::option::Option::None
                    }
                    _ => ::core::option::Option::None,
                }
            }

            fn focus_next(
                &mut self,
                cx: &mut ::ferritic_core::EventCtx,
                forward: bool,
                text: &mut dyn ::ferritic_core::TextEngine,
            ) {
                let mut focusable = ::std::vec::Vec::new();
                for node in Self::ALL_NODES.iter() {
                    if self.is_focusable(*node) {
                        focusable.push(*node);
                    }
                }
                if focusable.is_empty() {
                    return;
                }
                let current = cx.focus.and_then(Self::node_from_u32);
                let index = current.and_then(|node| focusable.iter().position(|other| *other == node));
                let next = match index {
                    Some(index) => {
                        if forward {
                            (index + 1) % focusable.len()
                        } else {
                            (index + focusable.len() - 1) % focusable.len()
                        }
                    }
                    None => {
                        if forward {
                            0
                        } else {
                            focusable.len() - 1
                        }
                    }
                };
                self.change_focus(cx, Some(focusable[next] as u32), text);
            }
        }
    }
}
