use crate::{Error, Result};
use crate::core::reader::{WasmReadable, WasmReader};
use crate::core::reader::span::Span;

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub enum SectionTy {
    Custom = 0,
    Type = 1,
    Import = 2,
    Function = 3,
    Table = 4,
    Memory = 5,
    Global = 6,
    Export = 7,
    Start = 8,
    Element = 9,
    Code = 10,
    Data = 11,
    DataCount = 12,
}

impl WasmReadable for SectionTy {
    fn read(wasm: &mut WasmReader) -> Result<Self> {
        use SectionTy::*;
        let ty = match wasm.read_u8()? {
            0 => Custom,
            1 => Type,
            2 => Import,
            3 => Function,
            4 => Table,
            5 => Memory,
            6 => Global,
            7 => Export,
            8 => Start,
            9 => Element,
            10 => Code,
            11 => Data,
            12 => DataCount,
            other => return Err(Error::InvalidSectionType(other)),
        };

        Ok(ty)
    }
}

#[derive(Debug)]
pub(crate) struct SectionHeader {
    pub ty: SectionTy,
    pub contents: Span,
}

impl WasmReadable for SectionHeader {
    fn read(wasm: &mut WasmReader) -> Result<Self> {
        let ty = SectionTy::read(wasm)?;
        let size: u32 = wasm.read_var_u32()?;
        let contents_span = wasm.make_span(size as usize);

        Ok(SectionHeader {
            ty,
            contents: contents_span,
        })
    }
}