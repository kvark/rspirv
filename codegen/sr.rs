// Copyright 2017 Google Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::structs;
use crate::utils::*;

use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;

/// Returns the corresponding Rust type used in structured representation
/// for the given operand kind in the SPIR-V JSON grammar.
pub fn get_operand_type_sr_tokens(kind: &str) -> TokenStream {
    match kind {
        "IdMemorySemantics" | "IdScope" | "IdRef" | "IdResult" => quote! { spirv::Word },
        "LiteralInteger" | "LiteralExtInstInteger" => quote! { u32 },
        "LiteralSpecConstantOpInteger" => quote! { spirv::Op },
        "LiteralContextDependentNumber" => panic!("this kind is not expected to be handled here"),
        "LiteralString" => quote! { String },
        "PairLiteralIntegerIdRef" => quote! { (u32, spirv::Word) },
        "PairIdRefLiteralInteger" => quote! { (spirv::Word, u32) },
        "PairIdRefIdRef" => quote! { (spirv::Word, spirv::Word) },
        _ => {
            let kind = Ident::new(kind, Span::call_site());
            quote! { spirv::#kind }
        }
    }
}

/// Returns the corresponding Rust name used in structured representation
/// for the given operand kind in the SPIR-V JSON grammar.
pub fn get_operand_name_sr_tokens(param: &structs::Operand) -> TokenStream {
    let name = get_param_name(param);
    let token = Ident::new(&name, Span::call_site());
    quote! { #token }
}

pub fn gen_sr_decoration(grammar: &structs::Grammar) -> String {
    // The decoration operand kind
    let decoration = grammar
        .operand_kinds
        .iter()
        .find(|k| k.kind == "Decoration")
        .unwrap();
    // Go and compose all its enumerants
    let enumerants: Vec<_> = decoration
        .enumerants
        .iter()
        .map(|enumerant| {
            // Parameters for this enumerant
            let types: Vec<_> = enumerant
                .parameters
                .iter()
                .map(|p| get_operand_type_sr_tokens(&p.kind))
                .collect();
            let params = if types.is_empty() {
                quote!{}
            } else {
                quote! { (#( #types ),*) }
            };
            let symbol = Ident::new(enumerant.symbol.as_str(), Span::call_site());
            quote! { #symbol #params }
        })
        .collect();
    let tokens = quote! {
        use derive_more::From;
        use spirv;

        /// SPIR-V decorations.
        #[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, From)]
        pub enum Decoration {
            #( #enumerants ),*
        }
    };
    tokens.to_string()
}

pub fn get_quantified_type_tokens(ty: TokenStream, quantifier: &str) -> TokenStream {
    match quantifier {
        "" => quote! { #ty },
        "?" => quote! { Option<#ty> },
        "*" => quote! { Vec<#ty> },
        other => panic!("wrong quantifier: {}", other),
    }
}

pub fn get_operand_type_ident(operand: &structs::Operand) -> TokenStream {
    let ty = if operand.kind == "IdRef" {
        match operand.name.trim_matches('\'') {
            "Length" => quote! { Token<super::Constant> },
            "Entry Point" => quote! { Token<super::Function> },
            "Interface" => quote! { Token<super::Variable> },
            "Function Type" => quote! { Token<super::types::Function> },
            "Field Types" => quote! { super::types::StructMember },
            _ => quote! { Token<super::types::Type> },
        }
    } else {
        get_operand_type_sr_tokens(&operand.kind)
    };

    get_quantified_type_tokens(ty, &operand.quantifier)
}

fn get_type_fn_name(name: &str) -> String {
    if name == "Struct" {
        "structure".to_string()
    } else {
        snake_casify(name)
    }
}

const STANDALONE_TYPES: &[&str] = &[
    "Function",
];

pub fn gen_sr_types_checks_and_lifts(grammar: &structs::Grammar) -> (String, String, String) {
    let mut structs = Vec::new();
    let mut lifts = Vec::new();
    let mut variants = Vec::new();
    let mut checks = Vec::new();

    for inst in grammar.instructions
        .iter()
        .filter(|inst| inst.class == "Type")
    {
        let symbol = &inst.opname[6..];
        let is_empty = inst.operands.len() <= 1;
        let symbol_ident = Ident::new(symbol, Span::call_site());
        let opcode = inst.opcode;
        let ident_operands = Ident::new("operands", Span::call_site());

        let mut variant_declarations = Vec::new();
        let mut struct_declarations = Vec::new();
        let mut definitions = Vec::new();

        for op in inst.operands[1 ..].iter() {
            let field_name = get_operand_name_sr_tokens(op);
            let field_type = get_operand_type_ident(op);
            let constructor = lift_operand_complex(&ident_operands, op);

            variant_declarations.push(quote! {
                #field_name: #field_type,
            });
            struct_declarations.push(quote! {
                pub #field_name: #field_type,
            });
            definitions.push(quote! {
                #field_name : #constructor,
            });
        };

        if STANDALONE_TYPES.contains(&symbol) {
            structs.push(quote! {
                #[derive(Clone, Debug, PartialEq, Eq)]
                pub struct #symbol_ident {
                    pub decorations: Vec<super::Decoration>,
                    #( #struct_declarations )*
                }
            });
            let method_name = Ident::new(
                &format!("lift_type_{}", snake_casify(symbol)),
                Span::call_site(),
            );
            let oper_iter = if definitions.is_empty() {
                quote! {}
            } else {
                quote! {
                    let mut #ident_operands = raw.operands.iter()
                }
            };
            lifts.push(quote! {
                impl Context {
                    pub fn #method_name(
                        &mut self, raw: &mr::Instruction
                    ) -> Result<types::#symbol_ident, LiftError> {
                        if raw.class.opcode as u32 != #opcode {
                            return Err(LiftError::OpCode)
                        }
                        #oper_iter;
                        Ok(types::#symbol_ident {
                            decorations: Vec::new(), //TODO
                            #( #definitions )*
                        })
                    }
                }
            });
        } else {
            let variant_params = if is_empty {
                quote!{}
            } else {
                quote! { { #( #variant_declarations )* } }
            };
            variants.push(quote! {
                #symbol_ident #variant_params
            });
            let func_name = Ident::new(
                &format!("is_{}_type", get_type_fn_name(symbol)),
                Span::call_site(),
            );
            // If the type requires parameters, attach `{ .. }` to the match arm.
            let check_params = if is_empty {
                quote!{}
            } else {
                quote! { {..} }
            };
            checks.push(quote! {
                pub fn #func_name(&self) -> bool {
                    match self.ty {
                        TypeEnum::#symbol_ident #check_params => true,
                        _ => false
                    }
                }
            });
        }
    }

    let enums = quote! {
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub (in crate::sr) enum TypeEnum {
            #( #variants ),*
        }

        impl Type {
            #( #checks )*
        }
    };
    let structs = quote!( #( #structs )* );
    let lifts = quote!( #( #lifts )* );
    (enums.to_string(), structs.to_string(), lifts.to_string())
}

pub fn gen_sr_type_creation(grammar: &structs::Grammar) -> String {
    // Collect all types and their parameters in the following format:
    //   (type-name: &str, Vec<(param-name: quote::Ident, param-type: quote::Ident)>)
    let cases: Vec<_> = grammar
        .instructions
        .iter()
        .filter(|k| k.class == "Type")
        .filter(|k| k.opname != "OpTypeStruct" && !STANDALONE_TYPES.contains(&&k.opname[6..]))
        .map(|kind| {
            let operands: Vec<_> = kind
                .operands
                .iter()
                .skip(1)
                .map(|op| {
                    let name = Ident::new(&get_param_name(op), Span::call_site());
                    let ty = get_operand_type_ident(op);
                    (name, ty)
                })
                .collect();
            let symbol = &kind.opname[6..];
            (symbol, operands)
        })
        .collect();
    let constructors: Vec<_> = cases
        .iter()
        .map(|&(symbol, ref params)| {
            let func_name = Ident::new(
                &format!("type_{}", get_type_fn_name(symbol)),
                Span::call_site(),
            );
            let symbol = Ident::new(symbol, Span::call_site());
            let param_list: Vec<_> = params
                .iter()
                .map(|&(ref name, ref ty)| {
                    quote! { #name : #ty }
                })
                .collect();
            let param_list = quote! { (&mut self, #( #param_list ),*) };
            let init_list: Vec<_> = params
                .iter()
                .map(|&(ref name, _)| {
                    quote! { #name : #name }
                })
                .collect();
            let init_list = if init_list.is_empty() {
                quote!{}
            } else {
                quote! { {#( #init_list ),*} }
            };
            quote! {
                pub fn #func_name #param_list -> Token<Type> {
                    self.types.fetch_or_append(Type {
                        ty: TypeEnum::#symbol #init_list,
                        decorations: Vec::new(),
                    })
                }
            }
        })
        .collect();
    let tokens = quote! {
        impl Context {
            #( #constructors )*
        }
    };
    tokens.to_string()
}

fn lift_operand_simple(iter: &Ident, operand: &structs::Operand) -> TokenStream {
    let kind_ident = Ident::new(&operand.kind, Span::call_site());
    match operand.kind.as_str() {
        "PairLiteralIntegerIdRef" |
        "PairIdRefLiteralInteger" |
        "PairIdRefIdRef" => quote! {
        },
        _ => {
            let value = match operand.name.trim_matches('\'') {
                // structures support per-member decorations
                "Field Types" => quote! { super::types::StructMember::new(value.clone()) },
                _ if &operand.kind == "IdRef" => quote! { Token::new(*value) },
                _ => quote! { value.clone() },
            };
            quote! {
                match #iter.next() {
                    Some(&mr::Operand::#kind_ident(ref value)) => Some(#value),
                    Some(_) => Err(OperandError::Wrong)?,
                    None => None,
                }
            }
        }
    }
}

fn lift_operand_complex(iter: &Ident, operand: &structs::Operand) -> TokenStream {
    let value_token = lift_operand_simple(iter, operand);
    match operand.quantifier.as_str() {
        "" => quote! {
            (#value_token).ok_or(OperandError::Missing)?
        },
        "?" => value_token,
        "*" => quote! {
            {
                let mut vec = Vec::new();
                while let Some(value) = #value_token {
                    vec.push(value);
                }
                vec
            }
        },
        other => panic!("wrong quantifier: {}", other),
    }
}

pub fn gen_sr_structs_and_lifts(grammar: &structs::Grammar) -> (String, String) {
    let mut structs = Vec::new();
    let mut lifts = Vec::new();
    for inst in grammar.instructions.iter() {
        match inst.class.as_str() {
            "ModeSetting" |
            "ExtensionDecl" |
            "FunctionStruct" => (),
            _ => continue,
        }
        // Get the token for its enumerant
        let struct_name = Ident::new(&inst.opname[2..], Span::call_site());
        let ident_operands = Ident::new("operands", Span::call_site());
        let mut declarations = Vec::new();
        let mut definitions = Vec::new();

        for operand in inst.operands.iter() {
            if operand.kind.starts_with("IdResult") {
                continue
            }
            let field_name = get_operand_name_sr_tokens(operand);
            let field_type = get_operand_type_ident(operand);
            let constructor = lift_operand_complex(&ident_operands, operand);

            declarations.push(quote! {
                pub #field_name: #field_type,
            });
            definitions.push(quote! {
                #field_name : #constructor,
            });
        }
        
        let opcode = inst.opcode;
        let method_name = Ident::new(
            &format!("lift_{}", snake_casify(&inst.opname[2..])),
            Span::call_site(),
        );
        let oper_iter = if definitions.is_empty() {
            quote! {}
        } else {
            quote! {
                let mut #ident_operands = raw.operands.iter()
            }
        };

        structs.push(quote! {
            #[derive(Clone, Debug, Eq, PartialEq)]
            pub struct #struct_name {
                #( #declarations )*
            }
        });
        lifts.push(quote! {
            impl Context {
                pub fn #method_name(
                    &mut self, raw: &mr::Instruction
                ) -> Result<structs::#struct_name, LiftError> {
                    if raw.class.opcode as u32 != #opcode {
                        return Err(LiftError::OpCode)
                    }
                    #oper_iter;
                    Ok(structs::#struct_name {
                        #( #definitions )*
                    })
                }
            }
        });
    };

    let structs = quote!( #( #structs )* );
    let lifts = quote!( #( #lifts )* );
    (structs.to_string(), lifts.to_string())
}

pub fn gen_sr_instruction(grammar: &structs::Grammar) -> String {
    let mut terminators = Vec::new();
    let mut instructions = Vec::new();

    // Compose the token stream for all instructions
    for inst in grammar
        .instructions
        .iter() // Loop over all instructions
        .filter(|i| match i.class.as_str() {
            "Type" | "Constant" => false, // already done
            "ModeSetting" |
            "ExtensionDecl" |
            "FunctionStruct" => false, // already done in `gen_sr_structs_and_lifts`
            _ => true,
        })
    {
        // Get the token for its enumerant
        let name = Ident::new(&inst.opname[2..], Span::call_site());

        // Compose the token stream for all parameters
        let params: Vec<_> = inst.operands
            .iter() // Loop over all parameters
            .filter_map(|operand| {
                if operand.kind.starts_with("IdResult") {
                    None
                } else {
                    let field_name = get_operand_name_sr_tokens(operand);
                    let field_type = get_operand_type_sr_tokens(&operand.kind);
                    let quantified = get_quantified_type_tokens(field_type, &operand.quantifier);
                    Some(quote! { #field_name : #quantified })
                }
            }).collect();

        match inst.class.as_str() {
            "Terminator" => {
                terminators.push(quote! {
                    #name {#( #params ),*}
                });
            }
            _ => {
                let params = if params.is_empty() {
                    quote!{}
                } else {
                    quote! { {#( #params ),*} }
                };
                instructions.push(quote! { #name #params });
            }
        }
    }

    // Wrap it up with enum definition boilerplate
    let all = quote! {
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub enum Terminator {
            #( #terminators ),*
        }

        #[derive(Clone, Debug, Eq, PartialEq)]
        pub enum Instruction {
            #( #instructions ),*
        }
    };
    all.to_string()
}
