use crate::{
    mr,
    spirv,
};
use super::{
    context::{Context, Token},
    instructions::{Terminator},
    structs,
    types::Type,
    LiftError,
};


#[derive(Debug)]
pub struct Variable {
    //TODO
}

#[derive(Debug)]
pub struct BasicBlock {
    //line: Line,
    terminator: Terminator,
}

#[derive(Debug)]
pub struct Function {
    pub entry_point: Option<(structs::EntryPoint, structs::ExecutionMode)>,
    pub control: spirv::FunctionControl,
    /// Function result type.
    pub result: Token<Type>,
    /// Function parameters.
    pub parameters: Vec<Token<Type>>,
    /// Basic blocks in this function.
    pub basic_blocks: Vec<BasicBlock>,
}

pub struct Module {
    /// The module header.
    pub header: mr::ModuleHeader,
    /// All OpCapability instructions.
    pub capabilities: Vec<structs::Capability>,
    /// All OpExtension instructions.
    //pub extensions: Vec<structs::Extension>,
    /// All OpExtInstImport instructions.
    //pub ext_inst_imports: Vec<structs::ExtInstImport>,
    /// The OpMemoryModel instruction.
    pub memory_model: structs::MemoryModel,

    // some missing here...

    /// All functions.
    pub functions: Vec<Function>,
}

#[derive(Clone, Debug)]
pub enum ConvertionError {
    MissingHeader,
    MissingFunction,
    MissingFunctionType,
    Lift(LiftError),
}

impl From<LiftError> for ConvertionError {
    fn from(error: LiftError) -> Self {
        ConvertionError::Lift(error)
    }
}

impl Module {
    pub fn from_data(module: &mr::Module) -> Result<Self, ConvertionError> {
        let mut context = Context::new();
        let mut functions = Vec::new();

        for fun in module.functions.iter() {
            let def = match fun.def {
                Some(ref instruction) => context.lift_function(instruction)?,
                None => return Err(ConvertionError::MissingFunction),
            };
            let fty = match module.types_global_values
                .iter()
                .find(|inst| inst.result_id == Some(def.function_type.id_ref()))
            {
                Some(inst) => context.lift_type_function(inst)?,
                None => return Err(ConvertionError::MissingFunctionType),
            };
            functions.push(Function {
                entry_point: None,
                control: def.function_control,
                result: fty.return_type,
                parameters: fty.parameter_types,
                basic_blocks: Vec::new(),
            });
        }

        Ok(Module {
            header: match module.header {
                Some(ref header) => header.clone(),
                None => return Err(ConvertionError::MissingHeader),
            },
            capabilities: module.capabilities
                .iter()
                .map(|cap| context.lift_capability(cap))
                .collect::<Result<_, LiftError>>()?,
            memory_model: match module.memory_model {
                Some(ref mm) => context.lift_memory_model(mm)?,
                None => return Err(ConvertionError::MissingHeader),
            },
            functions,
        })
    }
}
