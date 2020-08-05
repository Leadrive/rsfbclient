use crate::{ibase, statement::StatementData, xsqlda::XSqlDa, Connection, FbError};

use bytes::{BufMut, Bytes, BytesMut};
use ParamType::*;

/// Maximum parameter data length
const MAX_DATA_LENGTH: u16 = 32767;

/// Stores the data needed to send the parameters
pub struct Params {
    /// Input xsqlda
    pub(crate) xsqlda: Option<XSqlDa>,

    /// Data used by the xsqlda above
    _buffers: Vec<ParamBuffer>,
}

impl Params {
    /// Validate and set the parameters of a statement
    pub(crate) fn new(
        conn: &Connection,
        stmt: &mut StatementData,
        infos: Vec<ParamInfo>,
    ) -> Result<Self, FbError> {
        todo!();

        // let ibase = &conn.ibase;
        // let status = &conn.status;

        // let params = if !infos.is_empty() {
        //     let mut xsqlda = XSqlDa::new(infos.len() as i16);

        //     let ok = unsafe {
        //         ibase.isc_dsql_describe_bind()(
        //             status.borrow_mut().as_mut_ptr(),
        //             &mut stmt.handle,
        //             1,
        //             &mut *xsqlda,
        //         )
        //     };
        //     if ok != 0 {
        //         return Err(status.borrow().as_error(ibase));
        //     }

        //     if xsqlda.sqld != xsqlda.sqln {
        //         return Err(FbError::Other(format!(
        //             "Wrong parameter count, you passed {}, but the sql contains needs {} params",
        //             xsqlda.sqln, xsqlda.sqld
        //         )));
        //     }

        //     let buffers = infos
        //         .into_iter()
        //         .enumerate()
        //         .map(|(col, info)| {
        //             ParamBuffer::from_parameter(info, xsqlda.get_xsqlvar_mut(col).unwrap())
        //         })
        //         .collect();

        //     Self {
        //         _buffers: buffers,
        //         xsqlda: Some(xsqlda),
        //     }
        // } else {
        //     Self {
        //         _buffers: vec![],
        //         xsqlda: None,
        //     }
        // };

        // Ok(params)
    }

    /// For use when there is no statement, cant verify the number of parameters ahead of time
    pub fn new_immediate(infos: Vec<ParamInfo>) -> Self {
        todo!();

        // if !infos.is_empty() {
        //     let mut xsqlda = XSqlDa::new(infos.len() as i16);

        //     xsqlda.sqld = xsqlda.sqln;

        //     let buffers = infos
        //         .into_iter()
        //         .enumerate()
        //         .map(|(col, info)| {
        //             ParamBuffer::from_parameter(info, xsqlda.get_xsqlvar_mut(col).unwrap())
        //         })
        //         .collect();

        //     Self {
        //         _buffers: buffers,
        //         xsqlda: Some(xsqlda),
        //     }
        // } else {
        //     Self {
        //         _buffers: vec![],
        //         xsqlda: None,
        //     }
        // }
    }
}

/// Data for the parameters to send in the wire
pub struct ParamsBlr {
    /// Definitions of the data types
    pub blr: Bytes,
    /// Actual values of the data
    pub values: Bytes,
}

/// Convert the parameters to a blr (binary representation)
pub fn params_to_blr(params: &[ParamInfo]) -> Result<ParamsBlr, FbError> {
    let mut blr = BytesMut::with_capacity(256);
    let mut values = BytesMut::with_capacity(256);

    blr.put_slice(&[
        ibase::blr::VERSION5,
        ibase::blr::BEGIN,
        ibase::blr::MESSAGE,
        0, // Message index
    ]);
    // Message length, * 2 as there is 1 msg for the param type and another for the nullind
    blr.put_u16_le(params.len() as u16 * 2);

    for p in params {
        let len = p.buffer.len() as u16;
        if len > MAX_DATA_LENGTH {
            return Err("Parameter too big! Not supported yet".into());
        }

        values.put_slice(&p.buffer);
        if len % 4 != 0 {
            values.put_slice(&[0; 4][..4 - (len as usize % 4)])
        }
        values.put_i32_le(if p.null { -1 } else { 0 });

        match p.sqltype {
            ParamType::Text => {
                blr.put_u8(ibase::blr::TEXT);
                blr.put_u16_le(len);
            }

            ParamType::Integer => blr.put_slice(&[
                ibase::blr::INT64,
                0, // Scale
            ]),

            ParamType::Floating => blr.put_u8(ibase::blr::DOUBLE),

            ParamType::Timestamp => blr.put_u8(ibase::blr::TIMESTAMP),

            ParamType::Null => {
                // Represent as empty text
                blr.put_u8(ibase::blr::TEXT);
                blr.put_u16_le(0);
            }
        }
        // Nullind
        blr.put_slice(&[ibase::blr::SHORT, 0]);
    }

    blr.put_slice(&[ibase::blr::END, ibase::blr::EOC]);

    Ok(ParamsBlr {
        blr: blr.freeze(),
        values: values.freeze(),
    })
}

/// Data for the input XSQLVAR
pub struct ParamBuffer {
    /// Buffer for the parameter data
    _buffer: Box<[u8]>,

    /// Null indicator
    _nullind: Box<i16>,
}

impl ParamBuffer {
    /// Allocate a buffer from a value to use in an input (parameter) XSQLVAR
    pub fn from_parameter(mut info: ParamInfo, var: &mut ibase::XSQLVAR) -> Self {
        let null = if info.null { -1 } else { 0 };

        let mut nullind = Box::new(null);
        var.sqlind = &mut *nullind;

        var.sqltype = match info.sqltype {
            ParamType::Text => ibase::SQL_TEXT as i16 + 1,
            ParamType::Integer => ibase::SQL_INT64 as i16 + 1,
            ParamType::Floating => ibase::SQL_DOUBLE as i16 + 1,
            ParamType::Timestamp => ibase::SQL_TIMESTAMP as i16 + 1,
            ParamType::Null => ibase::SQL_NULL as i16 + 1,
        };
        var.sqlscale = 0;

        var.sqldata = info.buffer.as_mut_ptr() as *mut _;
        var.sqllen = info.buffer.len() as i16;

        ParamBuffer {
            _buffer: info.buffer,
            _nullind: nullind,
        }
    }
}

/// Data used to build the input XSQLVAR
pub struct ParamInfo {
    pub(crate) sqltype: ParamType,
    pub(crate) buffer: Box<[u8]>,
    pub(crate) null: bool,
}

pub enum ParamType {
    /// Send as text
    Text,

    /// Send as bigint
    Integer,

    /// Send as double
    Floating,

    // Send as timestamp
    Timestamp,

    // Send as null
    Null,
}

/// Implemented for types that can be sent as parameters
pub trait ToParam {
    fn to_info(self) -> ParamInfo;
}

impl ToParam for String {
    fn to_info(self) -> ParamInfo {
        let buffer = Vec::from(self).into_boxed_slice();

        ParamInfo {
            sqltype: Text,
            buffer,
            null: false,
        }
    }
}

impl ToParam for i64 {
    fn to_info(self) -> ParamInfo {
        let buffer = self.to_be_bytes().to_vec().into_boxed_slice();

        ParamInfo {
            sqltype: Integer,
            buffer,
            null: false,
        }
    }
}

/// Implements AsParam for integers
macro_rules! impl_param_int {
    ( $( $t: ident ),+ ) => {
        $(
            impl ToParam for $t {
                fn to_info(self) -> ParamInfo {
                    (self as i64).to_info()
                }
            }
        )+
    };
}

impl_param_int!(i32, u32, i16, u16, i8, u8);

impl ToParam for f64 {
    fn to_info(self) -> ParamInfo {
        let buffer = self.to_be_bytes().to_vec().into_boxed_slice();

        ParamInfo {
            sqltype: Floating,
            buffer,
            null: false,
        }
    }
}

impl ToParam for f32 {
    fn to_info(self) -> ParamInfo {
        (self as f64).to_info()
    }
}

/// Implements for all nullable variants
impl<T> ToParam for Option<T>
where
    T: ToParam,
{
    fn to_info(self) -> ParamInfo {
        if let Some(v) = self {
            v.to_info()
        } else {
            ParamInfo {
                sqltype: Null,
                buffer: Box::new([]),
                null: true,
            }
        }
    }
}

/// Implements for all borrowed variants (&str, Cow and etc)
impl<T, B> ToParam for &B
where
    B: ToOwned<Owned = T> + ?Sized,
    T: core::borrow::Borrow<B> + ToParam,
{
    fn to_info(self) -> ParamInfo {
        self.to_owned().to_info()
    }
}

/// Implemented for types that represents a list of parameters
pub trait IntoParams {
    fn to_params(self) -> Vec<ParamInfo>;
}

/// Represents no parameters
impl IntoParams for () {
    fn to_params(self) -> Vec<ParamInfo> {
        vec![]
    }
}

/// Generates IntoParams implementations for a tuple
macro_rules! impl_into_params {
    ($([$t: ident, $v: ident]),+) => {
        impl<$($t),+> IntoParams for ($($t,)+)
        where
            $( $t: ToParam, )+
        {
            fn to_params(self) -> Vec<ParamInfo> {
                let ( $($v,)+ ) = self;

                vec![ $(
                    $v.to_info(),
                )+ ]
            }
        }
    };
}

/// Generates FromRow implementations for various tuples
macro_rules! impls_into_params {
    ([$t: ident, $v: ident]) => {
        impl_into_params!([$t, $v]);
    };

    ([$t: ident, $v: ident], $([$ts: ident, $vs: ident]),+ ) => {
        impls_into_params!($([$ts, $vs]),+);

        impl_into_params!([$t, $v], $([$ts, $vs]),+);
    };
}

impls_into_params!(
    [A, a],
    [B, b],
    [C, c],
    [D, d],
    [E, e],
    [F, f],
    [G, g],
    [H, h],
    [I, i],
    [J, j],
    [K, k],
    [L, l],
    [M, m],
    [N, n],
    [O, o]
);
