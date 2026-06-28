// ── Dispatch ──

use std::collections::{HashMap, HashSet};
use std::path::Path;
use utencore_bytecode::ExportEntry;
use crate::error::{UtenError, UtenResult};
use crate::opcodes::{Opcode, OpFlags, opcode_info};
use utencore_types::*;
use super::*;

impl Vm {
    pub(crate) fn dispatch(&mut self, op: Opcode, operand: u32) -> UtenResult<()> {
        // Reconstruct bytecode slice from raw pointer (for opcodes that need inline data access)
        let bytecode: &[u8] = if self.current_bytecode_ptr.is_null() {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.current_bytecode_ptr, self.current_bytecode_len) }
        };
        use Opcode::*;
        match op {
            // ── 0x00–0x0F: Stack ──
            Nop => {}
            PushNil => self.stack.push(UValue::Nil),
            PushTrue => self.stack.push(UValue::Bool(true)),
            PushFalse => self.stack.push(UValue::Bool(false)),
            PushI32 => self.stack.push(UValue::Int32(operand as i32)),
            PushI64 => self.stack.push(UValue::Int64(operand as i64)),
            PushF32 => self.stack.push(UValue::Float32(operand as f32)),
            PushF64 => {
                let pos = self.pc as isize - 8;
                if pos >= 0 && (pos as usize) + 8 <= bytecode.len() {
                    let bits = u64::from_le_bytes(bytecode[pos as usize..pos as usize + 8].try_into().unwrap());
                    self.stack.push(UValue::Float64(f64::from_bits(bits)));
                } else {
                    self.stack.push(UValue::Float64(0.0));
                }
            }
            PushString => self.stack.push(UValue::String(operand as StringId)),
            PushConst => {
                let mid = self.current_module_id();
                let idx = operand as usize;
                if idx < self.modules[mid].module.constants.len() {
                    let c = &self.modules[mid].module.constants[idx];
                    match c {
                        crate::bytecode::ConstValue::Nil => self.stack.push(UValue::Nil),
                        crate::bytecode::ConstValue::Bool(b) => self.stack.push(UValue::Bool(*b)),
                        crate::bytecode::ConstValue::Int32(i) => self.stack.push(UValue::Int32(*i)),
                        crate::bytecode::ConstValue::Int64(i) => self.stack.push(UValue::Int64(*i)),
                        crate::bytecode::ConstValue::Float32(f) => self.stack.push(UValue::Float32(*f)),
                        crate::bytecode::ConstValue::Float64(f) => self.stack.push(UValue::Float64(*f)),
                        crate::bytecode::ConstValue::String(sid) => self.stack.push(UValue::String(*sid)),
                    }
                } else { self.stack.push(UValue::Nil); }
            }
            Dup => { let v = self.peek(0)?; self.stack.push(v.clone()); }
            DupN => {
                let n = operand as u8 as usize;
                for i in (0..n).rev() {
                    let v = self.peek(i)?;
                    self.stack.push(v.clone());
                }
            }
            Swap => { let a = self.pop()?; let b = self.pop()?; self.stack.push(a); self.stack.push(b); }
            Pop => { self.pop()?; }
            PopN => {
                let n = operand as usize;
                let len = self.stack.len();
                if n > len { return Err(UtenError::StackUnderflow { needed: n, actual: len }); }
                self.stack.truncate(len - n);
            }
            Rot => {
                if self.stack.len() < 3 { return Err(UtenError::StackUnderflow { needed: 3, actual: self.stack.len() }); }
                let len = self.stack.len();
                self.stack.swap(len - 1, len - 2);
                self.stack.swap(len - 2, len - 3);
                self.stack.swap(len - 1, len - 2);
            }

            // ── 0x10–0x1F: Arithmetic (Integer) ──
            Add => {
                // Check for operator overload via __add__
                let b_check = self.peek(0)?.clone();
                let a_check = self.peek(1)?.clone();
                if let Some(handler) = self.get_operator_handler(&a_check, "__add__") {
                    self.pop()?; self.pop()?;
                    return self.call_operator_handler(handler, vec![a_check, b_check]);
                }
                if let Some(handler) = self.get_operator_handler(&b_check, "__add__") {
                    self.pop()?; self.pop()?;
                    return self.call_operator_handler(handler, vec![a_check, b_check]);
                }
                let b = self.pop()?;
                let a = self.pop()?;
                match (&a, &b) {
                    (UValue::String(s1), UValue::String(s2)) => {
                        let mid = self.current_module_id();
                        let concat = format!("{}{}",
                            self.modules[mid].module.strings[*s1 as usize],
                            self.modules[mid].module.strings[*s2 as usize]);
                        let sid = self.modules[mid].module.intern(&concat);
                        self.stack.push(UValue::String(sid));
                    }
                    (UValue::Complex { real: ra, imag: ia }, UValue::Complex { real: rb, imag: ib }) => {
                        self.stack.push(UValue::Complex { real: ra + rb, imag: ia + ib });
                    }
                    (UValue::Gc(ha, ta), UValue::Gc(hb, tb))
                        if *ta == ValueTag::BigInt && *tb == ValueTag::BigInt =>
                    {
                        if let (HeapObject::BigInt(ba), HeapObject::BigInt(bb)) =
                            (self.gc.get(*ha), self.gc.get(*hb))
                        {
                            let r = HeapObject::BigInt(ba.clone() + bb.clone());
                            self.stack.push(UValue::Gc(self.gc.alloc(r), ValueTag::BigInt));
                        } else { self.stack.push(UValue::Nil); }
                    }
                    (UValue::Gc(ha, ta), other) if *ta == ValueTag::BigInt => {
                        if let HeapObject::BigInt(ba) = self.gc.get(*ha) {
                            if let Some(nb) = self.as_bigint(other) {
                                let r = HeapObject::BigInt(ba.clone() + nb);
                                self.stack.push(UValue::Gc(self.gc.alloc(r), ValueTag::BigInt));
                            } else { self.stack.push(UValue::Nil); }
                        } else { self.stack.push(UValue::Nil); }
                    }
                    (other, UValue::Gc(hb, tb)) if *tb == ValueTag::BigInt => {
                        if let HeapObject::BigInt(bb) = self.gc.get(*hb) {
                            if let Some(na) = self.as_bigint(other) {
                                let r = HeapObject::BigInt(na + bb.clone());
                                self.stack.push(UValue::Gc(self.gc.alloc(r), ValueTag::BigInt));
                            } else { self.stack.push(UValue::Nil); }
                        } else { self.stack.push(UValue::Nil); }
                    }
                    _ => {
                        if let (Ok(a), Ok(b)) = (self.value_as_int(&a), self.value_as_int(&b)) {
                            self.stack.push(UValue::Int64(a + b));
                        } else {
                            let a_str = self.value_to_string(&a);
                            let b_str = self.value_to_string(&b);
                            let concat = format!("{}{}", a_str, b_str);
                            let mid = self.current_module_id();
                            let sid = self.modules[mid].module.intern(&concat);
                            self.stack.push(UValue::String(sid));
                        }
                    }
                }
            }
            Sub => {
                if let Some(v) = self.try_bigint_binop(|a, b| a - b)? {
                    self.stack.push(v);
                } else {
                    let r = self.binary_int_op(|a, b| a - b)?;
                    self.stack.push(UValue::Int64(r));
                }
            }
            Mul => {
                let b = self.pop()?;
                let a = self.pop()?;
                match (&a, &b) {
                    (UValue::String(sid), UValue::Int32(n)) => {
                        let mid = self.current_module_id();
                        let s = &self.modules[mid].module.strings[*sid as usize];
                        let repeated = s.repeat(*n as usize);
                        let new_sid = self.modules[mid].module.intern(&repeated);
                        self.stack.push(UValue::String(new_sid));
                    }
                    (UValue::String(sid), UValue::Int64(n)) => {
                        let mid = self.current_module_id();
                        let s = &self.modules[mid].module.strings[*sid as usize];
                        let repeated = s.repeat(*n as usize);
                        let new_sid = self.modules[mid].module.intern(&repeated);
                        self.stack.push(UValue::String(new_sid));
                    }
                    (UValue::Gc(ha, ta), UValue::Gc(hb, tb)) if *ta == ValueTag::BigInt && *tb == ValueTag::BigInt => {
                        if let (HeapObject::BigInt(ba), HeapObject::BigInt(bb)) =
                            (self.gc.get(*ha), self.gc.get(*hb))
                        {
                            let r = HeapObject::BigInt(ba.clone() * bb.clone());
                            self.stack.push(UValue::Gc(self.gc.alloc(r), ValueTag::BigInt));
                        } else { self.stack.push(UValue::Nil); }
                    }
                    (UValue::Gc(ha, ta), _) if *ta == ValueTag::BigInt => {
                        if let HeapObject::BigInt(ba) = self.gc.get(*ha) {
                            if let Some(nb) = self.as_bigint(&b) {
                                let r = HeapObject::BigInt(ba.clone() * nb);
                                self.stack.push(UValue::Gc(self.gc.alloc(r), ValueTag::BigInt));
                            } else { self.stack.push(UValue::Nil); }
                        } else { self.stack.push(UValue::Nil); }
                    }
                    (_, UValue::Gc(hb, tb)) if *tb == ValueTag::BigInt => {
                        if let HeapObject::BigInt(bb) = self.gc.get(*hb) {
                            if let Some(na) = self.as_bigint(&a) {
                                let r = HeapObject::BigInt(na * bb.clone());
                                self.stack.push(UValue::Gc(self.gc.alloc(r), ValueTag::BigInt));
                            } else { self.stack.push(UValue::Nil); }
                        } else { self.stack.push(UValue::Nil); }
                    }
                    _ => {
                        let r = match (&a, &b) {
                            (UValue::Int32(av), UValue::Int32(bv)) => (*av as i64) * (*bv as i64),
                            (UValue::Int64(av), UValue::Int64(bv)) => av * bv,
                            (UValue::Int32(av), UValue::Int64(bv)) => (*av as i64) * bv,
                            (UValue::Int64(av), UValue::Int32(bv)) => av * (*bv as i64),
                            _ => return Err(UtenError::TypeError { expected: "numeric", actual: format!("{:?} * {:?}", a.tag(), b.tag()) }),
                        };
                        self.stack.push(UValue::Int64(r));
                    }
                }
            }
            Div => {
                // Check divisor before any operation
                let divisor = self.peek(0)?.clone();
                let is_zero = match &divisor {
                    UValue::Int32(i) => *i == 0,
                    UValue::Int64(i) => *i == 0,
                    UValue::Gc(h, tag) if *tag == ValueTag::BigInt => {
                        if let HeapObject::BigInt(bi) = self.gc.get(*h) { bi == &num_bigint::BigInt::from(0i64) }
                        else { false }
                    }
                    _ => false,
                };
                if is_zero { return Err(UtenError::Vm("division by zero".into())); }

                if let Some(v) = self.try_bigint_binop(|a, b| a / b)? {
                    self.stack.push(v);
                } else {
                    let r = self.binary_int_op(|a, b| a / b)?;
                    self.stack.push(UValue::Int64(r));
                }
            }
            Mod => {
                let divisor = self.peek(0)?.clone();
                let is_zero = match &divisor {
                    UValue::Int32(i) => *i == 0,
                    UValue::Int64(i) => *i == 0,
                    UValue::Gc(h, tag) if *tag == ValueTag::BigInt => {
                        if let HeapObject::BigInt(bi) = self.gc.get(*h) { bi == &num_bigint::BigInt::from(0i64) }
                        else { false }
                    }
                    _ => false,
                };
                if is_zero { return Err(UtenError::Vm("modulo by zero".into())); }

                if let Some(v) = self.try_bigint_binop(|a, b| a % b)? {
                    self.stack.push(v);
                } else {
                    let r = self.binary_int_op(|a, b| a % b)?;
                    self.stack.push(UValue::Int64(r));
                }
            }
            Neg => {
                if let Some(v) = self.try_bigint_unop(|a| -a)? { self.stack.push(v); }
                else { let v = self.pop_int()?; self.stack.push(UValue::Int64(-v)); }
            }
            Inc => {
                if let Some(v) = self.try_bigint_unop(|a| a + 1i64)? { self.stack.push(v); }
                else { let v = self.pop_int()?; self.stack.push(UValue::Int64(v + 1)); }
            }
            Dec => {
                if let Some(v) = self.try_bigint_unop(|a| a - 1i64)? { self.stack.push(v); }
                else { let v = self.pop_int()?; self.stack.push(UValue::Int64(v - 1)); }
            }
            Abs => {
                if let Some(v) = self.try_bigint_unop(|a| if a < num_bigint::BigInt::from(0i64) { -a } else { a })? { self.stack.push(v); }
                else { let v = self.pop_int()?; self.stack.push(UValue::Int64(v.abs())); }
            }
            Pow => {
                if let Some(v) = self.try_bigint_binop(|a, b| {
                    let exp = b.iter_u32_digits().next().unwrap_or(0);
                    if b.sign() == num_bigint::Sign::Minus || b.iter_u32_digits().count() > 1 { a } else { a.pow(exp) }
                })? { self.stack.push(v); }
                else { let b = self.pop_int()? as u32; let a = self.pop_int()?; self.stack.push(UValue::Int64(a.pow(b))); }
            }
            CheckedAdd => {
                if let Some(v) = self.try_bigint_binop(|a, b| a + b)? { self.stack.push(v); }
                else {
                    let b = self.pop_int()?;
                    let a = self.pop_int()?;
                    match a.checked_add(b) { Some(r) => self.stack.push(UValue::Int64(r)), None => return Err(UtenError::Vm("overflow".into())) }
                }
            }
            CheckedSub => {
                if let Some(v) = self.try_bigint_binop(|a, b| a - b)? { self.stack.push(v); }
                else {
                    let b = self.pop_int()?;
                    let a = self.pop_int()?;
                    match a.checked_sub(b) { Some(r) => self.stack.push(UValue::Int64(r)), None => return Err(UtenError::Vm("underflow".into())) }
                }
            }
            CheckedMul => {
                if let Some(v) = self.try_bigint_binop(|a, b| a * b)? { self.stack.push(v); }
                else {
                    let b = self.pop_int()?;
                    let a = self.pop_int()?;
                    match a.checked_mul(b) { Some(r) => self.stack.push(UValue::Int64(r)), None => return Err(UtenError::Vm("overflow".into())) }
                }
            }
            SaturatingAdd => { let b = self.pop_int()?; let a = self.pop_int()?; self.stack.push(UValue::Int64(a.saturating_add(b))); }
            SaturatingSub => { let b = self.pop_int()?; let a = self.pop_int()?; self.stack.push(UValue::Int64(a.saturating_sub(b))); }
            WrappingAdd => { let b = self.pop_int()?; let a = self.pop_int()?; self.stack.push(UValue::Int64(a.wrapping_add(b))); }

            // ── 0x20–0x2F: Floating-point Arithmetic ──
            FAdd => { let r = self.binary_float_op(|a, b| a + b)?; self.stack.push(UValue::Float64(r)); }
            FSub => { let r = self.binary_float_op(|a, b| a - b)?; self.stack.push(UValue::Float64(r)); }
            FMul => { let r = self.binary_float_op(|a, b| a * b)?; self.stack.push(UValue::Float64(r)); }
            FDiv => { let r = self.binary_float_op(|a, b| a / b)?; self.stack.push(UValue::Float64(r)); }
            FMod => { let r = self.binary_float_op(|a, b| a % b)?; self.stack.push(UValue::Float64(r)); }
            FNeg => { let v = self.pop_float()?; self.stack.push(UValue::Float64(-v)); }
            FPow => { let b = self.pop_float()?; let a = self.pop_float()?; self.stack.push(UValue::Float64(a.powf(b))); }
            FSqrt => { let v = self.pop_float()?; self.stack.push(UValue::Float64(v.sqrt())); }
            FAbs => { let v = self.pop_float()?; self.stack.push(UValue::Float64(v.abs())); }
            FFloor => { let v = self.pop_float()?; self.stack.push(UValue::Float64(v.floor())); }
            FCeil => { let v = self.pop_float()?; self.stack.push(UValue::Float64(v.ceil())); }
            FRound => { let v = self.pop_float()?; self.stack.push(UValue::Float64(v.round())); }
            FSin => { let v = self.pop_float()?; self.stack.push(UValue::Float64(v.sin())); }
            FCos => { let v = self.pop_float()?; self.stack.push(UValue::Float64(v.cos())); }
            FTan => { let v = self.pop_float()?; self.stack.push(UValue::Float64(v.tan())); }
            FAtan2 => { let b = self.pop_float()?; let a = self.pop_float()?; self.stack.push(UValue::Float64(a.atan2(b))); }

            // ── 0x30–0x3F: Bitwise ──
            BitAnd => { let b = self.pop_int()?; let a = self.pop_int()?; self.stack.push(UValue::Int64(a & b)); }
            BitOr => { let b = self.pop_int()?; let a = self.pop_int()?; self.stack.push(UValue::Int64(a | b)); }
            BitXor => { let b = self.pop_int()?; let a = self.pop_int()?; self.stack.push(UValue::Int64(a ^ b)); }
            BitNot => { let v = self.pop_int()?; self.stack.push(UValue::Int64(!v)); }
            Shl => { let b = self.pop_uint()?; let a = self.pop_int()?; self.stack.push(UValue::Int64(a.wrapping_shl(b as u32))); }
            Shr => { let b = self.pop_uint()?; let a = self.pop_int()?; self.stack.push(UValue::Int64(a.wrapping_shr(b as u32))); }
            UShr => { let b = self.pop_uint()?; let a = self.pop_uint()?; self.stack.push(UValue::Int64((a.wrapping_shr(b as u32)) as i64)); }
            RotLeft => { let b = self.pop_uint()?; let a = self.pop_int()?; self.stack.push(UValue::Int64(a.rotate_left(b as u32))); }
            RotRight => { let b = self.pop_uint()?; let a = self.pop_int()?; self.stack.push(UValue::Int64(a.rotate_right(b as u32))); }
            PopCount => { let v = self.pop_uint()?; self.stack.push(UValue::Int64(v.count_ones() as i64)); }
            LeadingZeros => { let v = self.pop_uint()?; self.stack.push(UValue::Int64(v.leading_zeros() as i64)); }
            TrailingZeros => { let v = self.pop_uint()?; self.stack.push(UValue::Int64(v.trailing_zeros() as i64)); }
            ByteSwap => { let v = self.pop_uint()?; self.stack.push(UValue::Int64(v.swap_bytes() as i64)); }
            BitReverse => { let v = self.pop_uint()?; self.stack.push(UValue::Int64(v.reverse_bits() as i64)); }
            UDiv => { let b = self.pop_uint()?; let a = self.pop_uint()?; if b != 0 { self.stack.push(UValue::Int64((a / b) as i64)); } else { return Err(UtenError::Vm("division by zero".into())); } }
            UMod => { let b = self.pop_uint()?; let a = self.pop_uint()?; if b != 0 { self.stack.push(UValue::Int64((a % b) as i64)); } else { return Err(UtenError::Vm("mod by zero".into())); } }

            // ── 0x40–0x4F: Comparison ──
            Eq => {
                let b_check = self.peek(0)?.clone();
                let a_check = self.peek(1)?.clone();
                if let Some(handler) = self.get_operator_handler(&a_check, "__eq__") {
                    self.pop()?; self.pop()?;
                    return self.call_operator_handler(handler, vec![a_check, b_check]);
                }
                if let Some(handler) = self.get_operator_handler(&b_check, "__eq__") {
                    self.pop()?; self.pop()?;
                    return self.call_operator_handler(handler, vec![a_check, b_check]);
                }
                let b = self.pop()?; let a = self.pop()?;
                self.stack.push(UValue::Bool(a == b));
            }
            Ne => { let b = self.pop()?; let a = self.pop()?; self.stack.push(UValue::Bool(a != b)); }
            Lt => {
                let b = self.pop()?; let a = self.pop()?;
                if let (Some(ba), Some(bb)) = (self.as_bigint(&a), self.as_bigint(&b)) {
                    self.stack.push(UValue::Bool(ba < bb));
                } else {
                    let (ai, bi) = (self.value_as_int(&a)?, self.value_as_int(&b)?);
                    self.stack.push(UValue::Bool(ai < bi));
                }
            }
            Le => {
                let b = self.pop()?; let a = self.pop()?;
                if let (Some(ba), Some(bb)) = (self.as_bigint(&a), self.as_bigint(&b)) {
                    self.stack.push(UValue::Bool(ba <= bb));
                } else {
                    let (ai, bi) = (self.value_as_int(&a)?, self.value_as_int(&b)?);
                    self.stack.push(UValue::Bool(ai <= bi));
                }
            }
            Gt => {
                let b = self.pop()?; let a = self.pop()?;
                if let (Some(ba), Some(bb)) = (self.as_bigint(&a), self.as_bigint(&b)) {
                    self.stack.push(UValue::Bool(ba > bb));
                } else {
                    let (ai, bi) = (self.value_as_int(&a)?, self.value_as_int(&b)?);
                    self.stack.push(UValue::Bool(ai > bi));
                }
            }
            Ge => {
                let b = self.pop()?; let a = self.pop()?;
                if let (Some(ba), Some(bb)) = (self.as_bigint(&a), self.as_bigint(&b)) {
                    self.stack.push(UValue::Bool(ba >= bb));
                } else {
                    let (ai, bi) = (self.value_as_int(&a)?, self.value_as_int(&b)?);
                    self.stack.push(UValue::Bool(ai >= bi));
                }
            }
            Cmp => {
                let b = self.pop()?; let a = self.pop()?;
                if let (Some(ba), Some(bb)) = (self.as_bigint(&a), self.as_bigint(&b)) {
                    self.stack.push(UValue::Int32(ba.cmp(&bb) as i32));
                } else {
                    let (ai, bi) = (self.value_as_int(&a)?, self.value_as_int(&b)?);
                    self.stack.push(UValue::Int32(ai.cmp(&bi) as i32));
                }
            }

            // ── 0x50–0x5F: Logic/Type ──
            Is => { let b = self.pop()?; let a = self.pop()?; self.stack.push(UValue::Bool(matches!((&a, &b), (UValue::Gc(x,_), UValue::Gc(y,_)) if x == y))); }
            IsNot => { let b = self.pop()?; let a = self.pop()?; self.stack.push(UValue::Bool(!matches!((&a, &b), (UValue::Gc(x,_), UValue::Gc(y,_)) if x == y))); }
            In => {
                let key = self.pop()?;
                let container = self.pop()?;
                let found = match container {
                    UValue::Gc(h, _) => match self.gc.get(h) {
                        HeapObject::Array(arr) => arr.contains(&key),
                        HeapObject::Map(map) => map.contains_key(&key),
                        HeapObject::Set(set) => set.contains(&key),
                        _ => false,
                    },
                    UValue::String(sid) => {
                        let mid = self.current_module_id();
                        let s = &self.modules[mid].module.strings[sid as usize].clone();
                        let pat = self.value_to_string(&key);
                        s.contains(&pat)
                    }
                    _ => false,
                };
                self.stack.push(UValue::Bool(found));
            }
            NotIn => { let key = self.pop()?; let container = self.pop()?;
                let found = match container {
                    UValue::Gc(h, _) => match self.gc.get(h) {
                        HeapObject::Array(arr) => arr.contains(&key),
                        HeapObject::Map(map) => map.contains_key(&key),
                        HeapObject::Set(set) => set.contains(&key),
                        _ => false,
                    },
                    _ => false,
                };
                self.stack.push(UValue::Bool(!found));
            }
            And => { let b = self.pop()?.truthy(); let a = self.pop()?.truthy(); self.stack.push(UValue::Bool(a && b)); }
            Or => { let b = self.pop()?.truthy(); let a = self.pop()?.truthy(); self.stack.push(UValue::Bool(a || b)); }
            Not => { let v = self.pop()?; self.stack.push(UValue::Bool(!v.truthy())); }
            Xor => { let b = self.pop()?.truthy(); let a = self.pop()?.truthy(); self.stack.push(UValue::Bool(a ^ b)); }
            Truthy => { let v = self.pop()?; self.stack.push(UValue::Bool(v.truthy())); }
            TypeOf => { let v = self.peek(0)?; self.stack.push(UValue::Int32(v.tag() as i32)); }
            IsType => { let tag_val = self.pop_int()?; let v = self.pop()?; self.stack.push(UValue::Bool(v.tag() as i32 == tag_val as i32)); }

            // ── 0x60–0x6F: Conversion ──
            ToI32 => { let v = self.pop_int()?; self.stack.push(UValue::Int32(v as i32)); }
            ToI64 => { let v = self.pop_int()?; self.stack.push(UValue::Int64(v)); }
            ToF32 => { let v = self.pop_float()?; self.stack.push(UValue::Float32(v as f32)); }
            ToF64 => { let v = self.pop_float()?; self.stack.push(UValue::Float64(v)); }
            ToBool => { let v = self.pop()?; self.stack.push(UValue::Bool(v.truthy())); }
            ToString => {
                let v = self.pop()?;
                let mid = self.current_module_id();
                let s = self.value_to_string(&v);
                let sid = self.modules[mid].module.intern(&s);
                self.stack.push(UValue::String(sid));
            }
            Cast => {
                let target_tag = self.pop_int()? as u8;
                let v = self.pop()?;
                match (v.tag() as u8, target_tag) {
                    (a, b) if a == b => self.stack.push(v),
                    (_, 2) => self.stack.push(UValue::Int32(self.value_as_int(&v)? as i32)),
                    (_, 3) => self.stack.push(UValue::Int64(self.value_as_int(&v)?)),
                    (_, 5) => { let v = self.pop_float()?; self.stack.push(UValue::Float64(v)); },
                    _ => return Err(UtenError::TypeError { expected: format!("type tag {target_tag}").leak(), actual: format!("{:?}", v.tag()) }),
                }
            }
            BitCast => {
                let v = self.pop()?;
                match v {
                    UValue::Int32(x) => self.stack.push(UValue::Float32(f32::from_bits(x as u32))),
                    UValue::Float32(f) => self.stack.push(UValue::Int32(f.to_bits() as i32)),
                    UValue::Int64(x) => self.stack.push(UValue::Float64(f64::from_bits(x as u64))),
                    UValue::Float64(f) => self.stack.push(UValue::Int64(f.to_bits() as i64)),
                    _ => self.stack.push(v),
                }
            }

            // ── 0x70–0x7F: Enum / Safety ──
            EnumCreate => { let tag = self.pop_int()? as u8; let payload = self.pop()?;
                let h = self.gc.alloc(HeapObject::Struct(vec![
                    (0, UValue::Int32(tag as i32)),
                    (1, payload),
                ]));
                self.stack.push(UValue::Gc(h, ValueTag::Struct));
            }
            EnumMatch => {
                let v = self.pop()?;
                match v {
                    UValue::Gc(h, ValueTag::Struct) => {
                        if let HeapObject::Struct(fields) = self.gc.get(h) {
                            if fields.len() >= 2 {
                                let tag = fields[0].1.clone();
                                let payload = fields[1].1.clone();
                                self.stack.push(tag);
                                self.stack.push(payload);
                            } else { self.stack.push(UValue::Int32(-1)); self.stack.push(UValue::Nil); }
                        } else { self.stack.push(UValue::Int32(-1)); self.stack.push(UValue::Nil); }
                    }
                    _ => { self.stack.push(UValue::Int32(-1)); self.stack.push(UValue::Nil); }
                }
            }
            CheckIndex => { let len = self.pop_int()?; let idx = self.pop_int()?;
                if idx < 0 || idx >= len {
                    return Err(UtenError::Vm(format!("index {idx} out of bounds for len {len}")));
                }
                self.stack.push(UValue::Int64(idx));
            }
            CheckType => { let expected_tag = self.pop_int()? as u8; let v = self.peek(0)?;
                if v.tag() as u8 != expected_tag {
                    return Err(UtenError::TypeError { expected: format!("tag {expected_tag}").leak(), actual: format!("{:?}", v.tag()) });
                }
            }
            TypeAssert => { let expected_tag = self.pop_int()? as u8; let v = self.pop()?;
                if v.tag() as u8 != expected_tag {
                    return Err(UtenError::TypeError { expected: format!("tag {expected_tag}").leak(), actual: format!("{:?}", v.tag()) });
                }
                self.stack.push(v);
            }
            Unreachable => { return Err(UtenError::Vm("executed unreachable opcode".into())); }

            // ── 0x80–0x8F: Control Flow ──
            Jump => { self.pc = (self.pc as i64 + operand as i16 as i64) as usize; }
            JumpIfFalse => { let cond = self.pop()?; if !cond.truthy() { self.pc = (self.pc as i64 + operand as i16 as i64) as usize; } }
            JumpIfTrue => { let cond = self.pop()?; if cond.truthy() { self.pc = (self.pc as i64 + operand as i16 as i64) as usize; } }
            JumpIfEq => { let b = self.pop()?; let a = self.pop()?; let eq = matches!((&a, &b), (UValue::Int32(x), UValue::Int32(y)) if x == y); if eq { self.pc = (self.pc as i64 + operand as i16 as i64) as usize; } }
            JumpIfNe => { let b = self.pop()?; let a = self.pop()?; let ne = !matches!((&a, &b), (UValue::Int32(x), UValue::Int32(y)) if x == y); if ne { self.pc = (self.pc as i64 + operand as i16 as i64) as usize; } }
            JumpTable => {
                let idx = self.pop_int()?;
                let pos = if operand as u8 == 0 { self.pc } else { self.pc };
                let count = operand as u16;
                if count > 0 && self.pc + (count as usize * 2) <= bytecode.len() {
                    let entry = if (idx as u16) < count {
                        u16::from_le_bytes([bytecode[self.pc + idx as usize * 2], bytecode[self.pc + idx as usize * 2 + 1]]) as i16
                    } else {
                        u16::from_le_bytes([bytecode[self.pc + (count as usize - 1) * 2], bytecode[self.pc + (count as usize - 1) * 2 + 1]]) as i16
                    };
                    self.pc = (self.pc as i64 + entry as i64) as usize;
                }
            }
            ForPrep => { self.pc = (self.pc as i64 + operand as i16 as i64) as usize; }
            ForStep => {
                let step = self.pop_int()?;
                let limit = self.pop_int()?;
                let var = self.pop_int()?;
                let next = var + step;
                if (step > 0 && next <= limit) || (step < 0 && next >= limit) {
                    self.stack.push(UValue::Int64(next));
                    self.stack.push(UValue::Int64(limit));
                    self.stack.push(UValue::Int64(step));
                } else {
                    self.pc = (self.pc as i64 + operand as i16 as i64) as usize;
                }
            }
            Loop => { self.pc = (self.pc as i64 + operand as i16 as i64) as usize; }
            Switch => {
                let val = self.pop_int()?;
                let count = operand as u16;
                if count > 0 && self.pc + (count as usize * 4) <= bytecode.len() {
                    for i in 0..count as usize {
                        let case_val = i32::from_le_bytes([bytecode[self.pc + i*4], bytecode[self.pc + i*4+1], bytecode[self.pc + i*4+2], bytecode[self.pc + i*4+3]]) as i64;
                        if case_val == val {
                            let offset = u16::from_le_bytes([bytecode[self.pc + count as usize * 4], bytecode[self.pc + count as usize * 4 + 1]]) as i16;
                            self.pc = (self.pc as i64 + offset as i64) as usize;
                            break;
                        }
                    }
                }
            }
            MatchCheck => { let pattern_tag = self.pop_int()?; let v = self.peek(0)?; self.stack.push(UValue::Bool(v.tag() as i32 == pattern_tag as i32)); }
            Bind => { let _val = self.pop()?; self.stack.push(UValue::Bool(true)); }

            // ── 0x90–0x9F: Iterator ──
            GetIter => {
                let container = self.pop()?;
                match container {
                    UValue::Gc(h, tag) => {
                        let iter = HeapObject::Iterator { container_handle: h, index: 0, container_tag: tag };
                        self.stack.push(UValue::Gc(self.gc.alloc(iter), ValueTag::Iterator));
                    }
                    UValue::String(sid) => {
                        let mid = self.current_module_id();
                        let s = &self.modules[mid].module.strings[sid as usize].clone();
                        let arr: Vec<UValue> = s.chars().map(|c| {
                            let cid = self.modules[mid].module.intern(&c.to_string());
                            UValue::String(cid)
                        }).collect();
                        let handle = self.gc.alloc(HeapObject::Array(arr));
                        let iter = HeapObject::Iterator { container_handle: handle, index: 0, container_tag: ValueTag::Array };
                        self.stack.push(UValue::Gc(self.gc.alloc(iter), ValueTag::Iterator));
                    }
                    _ => {
                        let iter = HeapObject::Iterator { container_handle: 0, index: usize::MAX, container_tag: container.tag() };
                        self.stack.push(UValue::Gc(self.gc.alloc(iter), ValueTag::Iterator));
                    }
                }
            }
            Next => {
                let iter_handle = self.pop_gc(ValueTag::Iterator)?;
                let operand_jump = operand as i16;
                let (should_advance, next_val) = {
                    let iter = self.gc.get(iter_handle);
                    match iter {
                        HeapObject::Iterator { container_handle, index, container_tag } => {
                            let idx = *index;
                            if idx == usize::MAX { (false, UValue::Nil) }
                            else {
                                let result = match container_tag {
                                    ValueTag::Array => {
                                        if let HeapObject::Array(arr) = self.gc.get(*container_handle) {
                                            if idx < arr.len() { (true, arr[idx].clone()) }
                                            else { (false, UValue::Nil) }
                                        } else { (false, UValue::Nil) }
                                    }
                                    ValueTag::Map => {
                                        if let HeapObject::Map(map) = self.gc.get(*container_handle) {
                                            let keys: Vec<UValue> = map.keys().cloned().collect();
                                            if idx < keys.len() {
                                                let key = keys[idx].clone();
                                                let val = map.get(&key).cloned().unwrap_or(UValue::Nil);
                                                let pair = HeapObject::Tuple(vec![key, val]);
                                                let ph = self.gc.alloc(pair);
                                                (true, UValue::Gc(ph, ValueTag::Tuple))
                                            } else { (false, UValue::Nil) }
                                        } else { (false, UValue::Nil) }
                                    }
                                    ValueTag::Set => {
                                        if let HeapObject::Set(set) = self.gc.get(*container_handle) {
                                            let elems: Vec<UValue> = set.iter().cloned().collect();
                                            if idx < elems.len() { (true, elems[idx].clone()) }
                                            else { (false, UValue::Nil) }
                                        } else { (false, UValue::Nil) }
                                    }
                                    _ => (false, UValue::Nil),
                                };
                                result
                            }
                        }
                        _ => (false, UValue::Nil),
                    }
                };
                if should_advance {
                    if let HeapObject::Iterator { ref mut index, .. } = self.gc.get_mut(iter_handle) { *index += 1; }
                    self.stack.push(next_val);
                } else {
                    self.pc = (self.pc as i64 + operand_jump as i64) as usize;
                }
            }

            // ── 0xA0–0xAF: Async ──
            Await => { self.stack.push(UValue::Nil); }
            AsyncCall => { let func_ref = operand as FuncRef; self.call_function(func_ref)?; }

            // ── 0xB0–0xBF: Function Calls ──
            Call => { let func_ref = operand as FuncRef; self.check_recursion()?; self.call_depth += 1; self.call_function(func_ref)?; }
            CallValue => {
                let func_val = self.pop()?;
                if matches!(func_val, UValue::Nil) {
                    let arg_count = self.pop_int()? as usize;
                    for _ in 0..arg_count { self.pop()?; }
                    self.stack.push(UValue::Nil);
                    return Ok(());
                }
                let arg_count = self.pop_int()? as usize;
                let mut args = Vec::with_capacity(arg_count);
                for _ in 0..arg_count { args.push(self.pop()?); }
                args.reverse();
                let mid = self.current_module_id();
                self.call_depth += 1;
                match func_val {
                    UValue::Gc(h, tag) if tag == ValueTag::Closure || tag == ValueTag::Lambda => {
                        let (func, module_id) = match self.gc.get(h) {
                            HeapObject::Closure { func, module_id, .. } => (*func, *module_id),
                            HeapObject::Lambda { func, module_id, .. } => (*func, *module_id),
                            _ => return Err(UtenError::TypeError { expected: "function".into(), actual: format!("{:?}", tag) }),
                        };
                        self.call_function_with_args(module_id as usize, func, args)?;
                    }
                    UValue::NativeFn(ref nh) => {
                        let fr = nh.ptr as FuncRef;
                        self.call_function_with_args(mid, fr, args)?;
                    }
                    // Class as callable: Dog() → create instance only.
                    // Constructor calling is the COMPILER's responsibility.
                    // The `constructor` field on Class exists for compilers to
                    // mark which method is the constructor, but the VM does NOT
                    // auto-call it. py2uc emits explicit bytecode to invoke
                    // __init__ after instance creation. ts2uc would do the same
                    // with its own constructor. No magic names in the VM.
                    UValue::Gc(h, ValueTag::Class) => {
                        let n_fields = {
                            let class = self.gc.get(h);
                            match class {
                                HeapObject::Class { fields, .. } => fields.len(),
                                _ => 0,
                            }
                        };
                        let obj = HeapObject::Object { class_handle: h, fields: vec![UValue::Nil; n_fields], proto: None };
                        let obj_h = self.gc.alloc(obj);
                        self.stack.push(UValue::Gc(obj_h, ValueTag::Object));
                    }
                    UValue::NativeFunc(idx) => {
                        let func_arc = self.native_funcs.get(idx as usize)
                            .ok_or(UtenError::Vm(format!("Invalid native func index {idx}")))?
                            .0.clone();
                        let result = (func_arc)(self, &args)?;
                        self.stack.push(result);
                    }
                    _ => return Err(UtenError::TypeError { expected: "function".into(), actual: format!("{:?}", func_val.tag()) }),
                }
            }
            CallMethod => {
                let method_name_id = operand as StringId;
                let mid = self.current_module_id();
                let _method_name = self.modules[mid].module.strings.get(method_name_id as usize)
                    .cloned().unwrap_or_default();
                let arg_count = self.pop_int()? as usize;
                let mut args = Vec::with_capacity(arg_count + 1);
                for _ in 0..arg_count { args.push(self.pop()?); }
                args.reverse();
                let obj = self.pop()?;
                args.insert(0, obj);
                self.call_depth += 1;
                match &args[0] {
                    UValue::Gc(h, ValueTag::Object) => {
                        if let HeapObject::Object { class_handle, .. } = self.gc.get(*h) {
                            if let HeapObject::Class { methods, .. } = self.gc.get(*class_handle) {
                                if let Some((_, fr)) = methods.iter().find(|(sid, _)| *sid == method_name_id) {
                                    self.call_function_with_args(mid, *fr, args)?;
                                    return Ok(());
                                }
                            }
                        }
                        self.stack.push(UValue::Nil);
                    }
                    _ => { self.stack.push(UValue::Nil); }
                }
            }
            TailCall => { let func_ref = operand as FuncRef; self.call_depth += 1; self.frames.pop(); self.call_function(func_ref)?; }
            TailCallValue => {
                let func_val = self.pop()?;
                let arg_count = self.pop_int()? as usize;
                let mut args = Vec::with_capacity(arg_count);
                for _ in 0..arg_count { args.push(self.pop()?); }
                args.reverse();
                let mid = self.current_module_id();
                self.call_depth += 1;
                self.frames.pop();
                match func_val {
                    UValue::Gc(h, tag) if tag == ValueTag::Closure || tag == ValueTag::Lambda => {
                        let (func, module_id) = match self.gc.get(h) {
                            HeapObject::Closure { func, module_id, .. } => (*func, *module_id),
                            HeapObject::Lambda { func, module_id, .. } => (*func, *module_id),
                            _ => (0, 0),
                        };
                        self.call_function_with_args(module_id as usize, func, args)?;
                    }
                    _ => self.stack.push(UValue::Nil),
                }
            }
            Invoke => { let nargs = operand as usize; for _ in 0..nargs { self.pop()?; } self.pop()?; self.stack.push(UValue::Nil); }
            SuperCall => { let _obj = self.pop()?; self.stack.push(UValue::Nil); }
            Apply => {
                let args_arr = self.pop()?;
                let func_val = self.pop()?;
                let mid = self.current_module_id();
                let mut args = match args_arr {
                    UValue::Gc(h, ValueTag::Array) => { if let HeapObject::Array(arr) = self.gc.get(h) { arr.clone() } else { vec![] } }
                    _ => vec![],
                };
                self.call_depth += 1;
                match func_val {
                    UValue::Gc(h, tag) if tag == ValueTag::Closure || tag == ValueTag::Lambda => {
                        let (func, module_id) = match self.gc.get(h) {
                            HeapObject::Closure { func, module_id, .. } => (*func, *module_id),
                            HeapObject::Lambda { func, module_id, .. } => (*func, *module_id),
                            _ => (0, 0),
                        };
                        self.call_function_with_args(module_id as usize, func, args)?;
                    }
                    _ => { self.stack.push(UValue::Nil); }
                }
            }

            // ── 0xC0–0xCF: Return ──
            Return => { self.call_depth = self.call_depth.saturating_sub(1); return self.do_return(None); }
            ReturnValue => { let val = self.pop()?; self.call_depth = self.call_depth.saturating_sub(1); return self.do_return(Some(val)); }
            ReturnMultiple => {
                let n = operand as u8 as usize;
                let mut vals = Vec::with_capacity(n);
                for _ in 0..n { vals.push(self.pop()?); }
                vals.reverse();
                self.call_depth = self.call_depth.saturating_sub(1);
                let frame = self.frames.pop().ok_or(UtenError::Vm("no frame".into()))?;
                self.stack.truncate(frame.stack_base);
                for v in vals { self.stack.push(v); }
                if self.frames.is_empty() { self.running = false; }
                else { self.pc = frame.return_pc; }
            }

            // ── 0xD0–0xDF: Functional & Coroutine ──
            MakeClosure => { let fr = operand as FuncRef; let obj = HeapObject::Closure { func: fr, captures: vec![], module_id: self.current_module_id() as ModuleId }; self.stack.push(UValue::Gc(self.gc.alloc(obj), ValueTag::Closure)); }
            Capture => { let val = self.pop()?; let fr = self.current_func_ref(); let mid = self.current_module_id();
                for frame in self.frames.iter_mut().rev() {
                    if frame.func_ref == fr && frame.module_id as usize == mid { frame.captures.push(val); break; }
                }
            }
            // ── Pair/List operations ──
            Cons => {
                let cdr = self.pop()?;
                let car = self.pop()?;
                let pair = HeapObject::Pair { car: Box::new(car), cdr: Box::new(cdr) };
                self.stack.push(UValue::Gc(self.gc.alloc(pair), ValueTag::Pair));
            }
            Car => {
                let val = self.pop()?;
                match val {
                    UValue::Gc(h, ValueTag::Pair) => {
                        if let HeapObject::Pair { car, .. } = self.gc.get(h) {
                            self.stack.push(*car.clone());
                        } else { self.stack.push(UValue::Nil); }
                    }
                    _ => self.stack.push(UValue::Nil),
                }
            }
            Cdr => {
                let val = self.pop()?;
                match val {
                    UValue::Gc(h, ValueTag::Pair) => {
                        if let HeapObject::Pair { cdr, .. } = self.gc.get(h) {
                            self.stack.push(*cdr.clone());
                        } else { self.stack.push(UValue::Nil); }
                    }
                    _ => self.stack.push(UValue::Nil),
                }
            }
            List => {
                let count = operand as usize;
                let base = self.stack.len().saturating_sub(count);
                let mut elements = Vec::with_capacity(count);
                for i in base..self.stack.len() {
                    elements.push(self.stack[i].clone());
                }
                self.stack.truncate(base);
                self.stack.push(UValue::Gc(self.gc.alloc(HeapObject::Array(elements)), ValueTag::Array));
            }
            IsList => {
                let val = self.pop()?;
                let is_list = matches!(&val,
                    UValue::Gc(_, ValueTag::Array | ValueTag::Pair | ValueTag::Tuple)
                );
                self.stack.push(UValue::Bool(is_list));
            }
            // ── Higher-order functions (NativeFunc only; bytecode closures
            //     deferred until VM has a proper continuation mechanism) ──
            MapFn => {
                let func_val = self.pop()?;
                let arr_h = self.pop_gc(ValueTag::Array)?;
                let elements = match self.gc.get(arr_h) {
                    HeapObject::Array(elems) => elems.clone(),
                    _ => vec![],
                };
                let mut results = Vec::with_capacity(elements.len());
                for elem in elements {
                    match &func_val {
                        UValue::NativeFunc(idx) => {
                            let cloned = self.native_funcs.get(*idx as usize).map(|f| f.0.clone());
                            if let Some(arc_fn) = cloned {
                                let result = (arc_fn)(self, &[elem])?;
                                results.push(result);
                            } else { results.push(UValue::Nil); }
                        }
                        // Bytecode closures: push Nil as placeholder (requires continuation support)
                        _ => { results.push(UValue::Nil); }
                    }
                }
                self.stack.push(UValue::Gc(self.gc.alloc(HeapObject::Array(results)), ValueTag::Array));
            }
            FilterFn => {
                let func_val = self.pop()?;
                let arr_h = self.pop_gc(ValueTag::Array)?;
                let elements = match self.gc.get(arr_h) {
                    HeapObject::Array(elems) => elems.clone(),
                    _ => vec![],
                };
                let mut results = Vec::with_capacity(elements.len());
                for elem in elements {
                    let keep = match &func_val {
                        UValue::NativeFunc(idx) => {
                            let cloned = self.native_funcs.get(*idx as usize).map(|f| f.0.clone());
                            if let Some(arc_fn) = cloned {
                                let result = (arc_fn)(self, &[elem.clone()])?;
                                result.truthy()
                            } else { true }
                        }
                        _ => true,
                    };
                    if keep { results.push(elem); }
                }
                self.stack.push(UValue::Gc(self.gc.alloc(HeapObject::Array(results)), ValueTag::Array));
            }
            ReduceFn => {
                let func_val = self.pop()?;
                let arr_h = self.pop_gc(ValueTag::Array)?;
                let elements = match self.gc.get(arr_h) {
                    HeapObject::Array(elems) => elems.clone(),
                    _ => vec![],
                };
                let init = self.pop()?;
                let mut acc = init;
                for elem in elements {
                    match &func_val {
                        UValue::NativeFunc(idx) => {
                            let cloned = self.native_funcs.get(*idx as usize).map(|f| f.0.clone());
                            if let Some(arc_fn) = cloned {
                                acc = (arc_fn)(self, &[acc, elem])?;
                            }
                        }
                        _ => {}
                    }
                }
                self.stack.push(acc);
            }
            // ── Function composition ──
            Compose => {
                let g = self.pop()?;
                let f = self.pop()?;
                // Store (f, g) as a Pair for potential reification.
                // Real function composition requires closure creation at runtime
                // which needs continuation support for bytecode closures.
                let pair = HeapObject::Pair { car: Box::new(f), cdr: Box::new(g) };
                self.stack.push(UValue::Gc(self.gc.alloc(pair), ValueTag::Pair));
            }
            // ── Lazy evaluation (thunks) ──
            Delay => {
                let func_val = self.pop()?;
                match &func_val {
                    UValue::Gc(h, tag) if *tag == ValueTag::Closure || *tag == ValueTag::Lambda => {
                        let (func, captures, module_id) = {
                            let obj = self.gc.get(*h);
                            match obj {
                                HeapObject::Closure { func, captures, module_id } => (*func, captures.clone(), *module_id),
                                HeapObject::Lambda { func, captures, module_id } => (*func, captures.clone(), *module_id),
                                _ => return Err(UtenError::TypeError { expected: "function".into(), actual: format!("{:?}", tag) }),
                            }
                        };
                        let thunk = HeapObject::Thunk {
                            evaluated: false,
                            value: Box::new(UValue::Nil),
                            func: Some((module_id, func)),
                            captures,
                        };
                        self.stack.push(UValue::Gc(self.gc.alloc(thunk), ValueTag::Thunk));
                    }
                    _ => { self.stack.push(func_val); }
                }
            }
            Force => {
                let val = self.pop()?;
                match val {
                    UValue::Gc(h, ValueTag::Thunk) => {
                        // Clone all data out of the thunk before any GC operations
                        let (evaluated, thunk_value, thunk_func, thunk_captures) = {
                            let thunk = self.gc.get(h);
                            match thunk {
                                HeapObject::Thunk { evaluated, value, func, captures } => {
                                    (*evaluated, value.clone(), *func, captures.clone())
                                }
                                _ => (false, Box::new(UValue::Nil), None, vec![]),
                            }
                        };
                        if evaluated {
                            self.stack.push(*thunk_value);
                        } else if let Some((mid, fr)) = thunk_func {
                            // Create a closure from the thunk's function and captures
                            let closure = HeapObject::Closure {
                                func: fr, captures: thunk_captures, module_id: mid,
                            };
                            let ch = self.gc.alloc(closure);
                            // Mark the thunk as evaluated (cache the closure)
                            if let HeapObject::Thunk { ref mut evaluated, ref mut value, .. } = self.gc.get_mut(h) {
                                *evaluated = true;
                                *value = Box::new(UValue::Gc(ch, ValueTag::Closure));
                            }
                            self.stack.push(UValue::Gc(ch, ValueTag::Closure));
                        } else {
                            self.stack.push(UValue::Nil);
                        }
                    }
                    _ => self.stack.push(val),
                }
            }
            LoadUpvalue => { let idx = operand as u8 as usize; if let Some(frame) = self.frames.last() { let val = frame.captures.get(idx).cloned().unwrap_or(UValue::Nil); self.stack.push(val); } else { self.stack.push(UValue::Nil); } }
            StoreUpvalue => { let idx = operand as u8 as usize; let val = self.pop()?; if let Some(frame) = self.frames.last_mut() { if idx < frame.captures.len() { frame.captures[idx] = val; } } }
            Curry => {
                let n_fixed = self.pop_int()? as usize;
                let func_val = self.pop()?;
                let mut fixed_args = Vec::with_capacity(n_fixed);
                for _ in 0..n_fixed { fixed_args.push(self.pop()?); }
                fixed_args.reverse();
                let fr = match &func_val {
                    UValue::Gc(h, tag) if *tag == ValueTag::Closure => {
                        if let HeapObject::Closure { func, module_id, .. } = self.gc.get(*h) {
                            let closure = HeapObject::Closure { func: *func, captures: fixed_args, module_id: *module_id };
                            UValue::Gc(self.gc.alloc(closure), ValueTag::Closure)
                        } else { UValue::Nil }
                    }
                    _ => UValue::Nil,
                };
                self.stack.push(fr);
            }

            // ── 0xE0–0xEF: Variables / Locals ──
            LoadLocal => { let idx = operand as u16; let frame = self.frames.last().unwrap(); let val = frame.locals.get(idx as usize).unwrap_or(&UValue::Nil).clone(); self.stack.push(val); }
            StoreLocal => { let idx = operand as u16; let val = self.pop()?; if let Some(frame) = self.frames.last_mut() { if (idx as usize) < frame.locals.len() { frame.locals[idx as usize] = val; } } }
            LoadCapture => { let idx = operand as u8 as usize; let frame = self.frames.last().unwrap(); let val = frame.captures.get(idx).cloned().unwrap_or(UValue::Nil); self.stack.push(val); }
            StoreCapture => { let idx = operand as u8 as usize; let val = self.pop()?; if let Some(frame) = self.frames.last_mut() { if idx < frame.captures.len() { frame.captures[idx] = val; } } }
            LoadGlobal => { let idx = operand as u16; let mid = self.current_module_id(); if (idx as usize) < self.modules[mid].globals.len() { self.stack.push(self.modules[mid].globals[idx as usize].clone()); } else { self.stack.push(UValue::Nil); } }
            StoreGlobal => { let idx = operand as u16; let val = self.pop()?; let mid = self.current_module_id(); if (idx as usize) >= self.modules[mid].globals.len() { self.modules[mid].globals.resize(idx as usize + 16, UValue::Nil); } self.modules[mid].globals[idx as usize] = val; }
            LoadDynGlobal => { let name_id = operand as StringId; let mid = self.current_module_id(); let name = self.modules[mid].module.strings.get(name_id as usize).cloned().unwrap_or_default(); let g = &self.modules[mid].module.globals; let pos = g.iter().position(|gd| gd.name == name); let val = pos.and_then(|i| self.modules[mid].globals.get(i)).cloned().unwrap_or(UValue::Nil); self.stack.push(val); }
            StoreDynGlobal => { let name_id = operand as StringId; let val = self.pop()?; let mid = self.current_module_id(); let name = self.modules[mid].module.strings.get(name_id as usize).cloned().unwrap_or_default(); let g = &self.modules[mid].module.globals; let pos = g.iter().position(|gd| gd.name == name); if let Some(i) = pos { if i < self.modules[mid].globals.len() { self.modules[mid].globals[i] = val; } } }
            AllocFrame => { if let Some(frame) = self.frames.last_mut() { let n = operand as usize; if n > self.config.frame_size as usize { return Err(UtenError::Vm(format!("frame size {n} exceeds limit {}", self.config.frame_size))); } frame.locals.resize(n, UValue::Nil); } }
            LoadArg => { let idx = operand as u8 as usize; let frame = self.frames.last().unwrap(); let val = frame.locals.get(idx).unwrap_or(&UValue::Nil).clone(); self.stack.push(val); }
            LoadModuleVar => { let idx = operand as u16; let mid = self.current_module_id(); let val = self.modules[mid].globals.get(idx as usize).cloned().unwrap_or(UValue::Nil); self.stack.push(val); }
            StoreModuleVar => { let idx = operand as u16; let val = self.pop()?; let mid = self.current_module_id(); if (idx as usize) < self.modules[mid].globals.len() { self.modules[mid].globals[idx as usize] = val; } }
            LoadUpvalueFrom => { let _idx = operand as u8 as usize; if self.frames.len() >= 2 { let parent = &self.frames[self.frames.len() - 2]; let val = parent.captures.get(0).cloned().unwrap_or(UValue::Nil); self.stack.push(val); } else { self.stack.push(UValue::Nil); } }
            StoreUpvalueTo => { let _idx = operand as u8 as usize; let val = self.pop()?; if self.frames.len() >= 2 { let parent_idx = self.frames.len() - 2; let parent = &mut self.frames[parent_idx]; if !parent.captures.is_empty() { parent.captures[0] = val; } } }
            This => { if let Some(frame) = self.frames.last() { let val = frame.locals.first().cloned().unwrap_or(UValue::Nil); self.stack.push(val); } else { self.stack.push(UValue::Nil); } }
            ArgCount => { let n = self.frames.last().map(|f| f.locals.len()).unwrap_or(0); self.stack.push(UValue::Int32(n as i32)); }

            // ── 0xF0–0xFF: Data Structures ──
            NewArray => {
                let count = operand as usize;
                let base = self.stack.len().saturating_sub(count);
                let mut elements = Vec::with_capacity(count);
                for i in base..self.stack.len() { elements.push(self.stack[i].clone()); }
                self.stack.truncate(base);
                self.stack.push(UValue::Gc(self.gc.alloc(HeapObject::Array(elements)), ValueTag::Array));
            }
            ArrayLen => {
                let val = self.pop()?;
                match val {
                    UValue::Gc(h, _) => {
                        let len = match self.gc.get(h) {
                            HeapObject::Array(arr) => arr.len(),
                            HeapObject::Tuple(t) => t.len(),
                            HeapObject::Map(m) => m.len(),
                            HeapObject::Set(s) => s.len(),
                            HeapObject::Bytes(b) | HeapObject::ByteArray(b) => b.len(),
                            HeapObject::HeapString(s) => s.len(),
                            _ => 0,
                        };
                        self.stack.push(UValue::Int64(len as i64));
                    }
                    UValue::String(sid) => {
                        let mid = self.current_module_id();
                        let s = &self.modules[mid].module.strings[sid as usize];
                        self.stack.push(UValue::Int64(s.len() as i64));
                    }
                    _ => self.stack.push(UValue::Int64(0)),
                }
            }
            ArrayGet => { let idx = self.pop_int()?; let h = self.pop_gc(ValueTag::Array)?; let val = match self.gc.get(h) { HeapObject::Array(arr) => arr.get(idx as usize).cloned().unwrap_or(UValue::Nil), _ => UValue::Nil, }; self.stack.push(val); }
            ArraySet => { let val = self.pop()?; let child_h = if let UValue::Gc(ch, _) = &val { Some(*ch) } else { None }; let idx = self.pop_int()?; let h = self.pop_gc(ValueTag::Array)?; if let HeapObject::Array(ref mut arr) = self.gc.get_mut(h) { if (idx as usize) < arr.len() { arr[idx as usize] = val; } } if let Some(ch) = child_h { self.gc.write_barrier(h, ch); } }
            ArrayPush => { let val = self.pop()?; let child_h = if let UValue::Gc(ch, _) = &val { Some(*ch) } else { None }; let h = self.pop_gc(ValueTag::Array)?; if let HeapObject::Array(ref mut arr) = self.gc.get_mut(h) { arr.push(val); } if let Some(ch) = child_h { self.gc.write_barrier(h, ch); } }
            ArrayPop => { let h = self.pop_gc(ValueTag::Array)?; let val = match self.gc.get_mut(h) { HeapObject::Array(ref mut arr) => arr.pop().unwrap_or(UValue::Nil), _ => UValue::Nil, }; self.stack.push(val); }
            ArrayUnshift => { let val = self.pop()?; let child_h = if let UValue::Gc(ch, _) = &val { Some(*ch) } else { None }; let h = self.pop_gc(ValueTag::Array)?; if let HeapObject::Array(ref mut arr) = self.gc.get_mut(h) { arr.insert(0, val); } if let Some(ch) = child_h { self.gc.write_barrier(h, ch); } }
            ArrayShift => { let h = self.pop_gc(ValueTag::Array)?; let val = match self.gc.get_mut(h) { HeapObject::Array(ref mut arr) => if !arr.is_empty() { Some(arr.remove(0)) } else { None }, _ => None, }.unwrap_or(UValue::Nil); self.stack.push(val); }
            ArrayInsert => { let idx = self.pop_int()?; let val = self.pop()?; let child_h = if let UValue::Gc(ch, _) = &val { Some(*ch) } else { None }; let h = self.pop_gc(ValueTag::Array)?; if let HeapObject::Array(ref mut arr) = self.gc.get_mut(h) { let pos = idx.max(0).min(arr.len() as i64) as usize; arr.insert(pos, val); } if let Some(ch) = child_h { self.gc.write_barrier(h, ch); } }
            ArrayRemove => { let idx = self.pop_int()?; let h = self.pop_gc(ValueTag::Array)?; if let HeapObject::Array(ref mut arr) = self.gc.get_mut(h) { if (idx as usize) < arr.len() { arr.remove(idx as usize); } } }
            ArraySlice => { let end = self.pop_int()?; let start = self.pop_int()?; let h = self.pop_gc(ValueTag::Array)?; let slice = match self.gc.get(h) { HeapObject::Array(arr) => { let s = 0; let e = end.min(arr.len() as i64).max(s as i64) as usize; arr[s..e].to_vec() } _ => vec![], }; self.stack.push(UValue::Gc(self.gc.alloc(HeapObject::Array(slice)), ValueTag::Array)); }
            ArrayConcat => { let b = self.pop()?; let a = self.pop()?; let combined = match (&a, &b) { (UValue::Gc(ha, _), UValue::Gc(hb, _)) => { let mut arr = match self.gc.get(*ha) { HeapObject::Array(a) => a.clone(), _ => vec![] }; let right = match self.gc.get(*hb) { HeapObject::Array(b) => b.clone(), _ => vec![] }; arr.extend(right); arr } _ => vec![], }; self.stack.push(UValue::Gc(self.gc.alloc(HeapObject::Array(combined)), ValueTag::Array)); }
            ArrayContains => { let val = self.pop()?; let h = self.pop_gc(ValueTag::Array)?; let found = match self.gc.get(h) { HeapObject::Array(arr) => arr.contains(&val), _ => false, }; self.stack.push(UValue::Bool(found)); }
            ArrayIndexOf => { let val = self.pop()?; let h = self.pop_gc(ValueTag::Array)?; let idx = match self.gc.get(h) { HeapObject::Array(arr) => arr.iter().position(|x| match (x, &val) { (UValue::Int32(a), UValue::Int32(b)) => a == b, (UValue::Int64(a), UValue::Int64(b)) => a == b, _ => false, }).map(|i| i as i64).unwrap_or(-1), _ => -1, }; self.stack.push(UValue::Int64(idx)); }
            ArraySort => { let h = self.pop_gc(ValueTag::Array)?; if let HeapObject::Array(ref mut arr) = self.gc.get_mut(h) { let mut vals: Vec<i64> = arr.iter().filter_map(|v| match v { UValue::Int32(i) => Some(*i as i64), UValue::Int64(i) => Some(*i), _ => None, }).collect(); vals.sort(); for (i, v) in vals.into_iter().enumerate() { if i < arr.len() { arr[i] = UValue::Int64(v); } } } self.stack.push(UValue::Gc(h, ValueTag::Array)); }
            ArrayReverse => { let h = self.pop_gc(ValueTag::Array)?; if let HeapObject::Array(ref mut arr) = self.gc.get_mut(h) { arr.reverse(); } self.stack.push(UValue::Gc(h, ValueTag::Array)); }
            NewMap => { self.stack.push(UValue::Gc(self.gc.alloc(HeapObject::Map(HashMap::new())), ValueTag::Map)); }
            MapGet => { let key = self.pop()?; let h = self.pop_gc(ValueTag::Map)?; let val = match self.gc.get(h) { HeapObject::Map(map) => map.get(&key).cloned().unwrap_or(UValue::Nil), _ => UValue::Nil, }; self.stack.push(val); }
            MapSet => { let val = self.pop()?; let key = self.pop()?; let key_h = if let UValue::Gc(ch, _) = &key { Some(*ch) } else { None }; let val_h = if let UValue::Gc(ch, _) = &val { Some(*ch) } else { None }; let h = self.pop_gc(ValueTag::Map)?; if let HeapObject::Map(ref mut map) = self.gc.get_mut(h) { map.insert(key, val); } if let Some(ch) = key_h { self.gc.write_barrier(h, ch); } if let Some(ch) = val_h { self.gc.write_barrier(h, ch); } }
            MapDel => { let key = self.pop()?; let h = self.pop_gc(ValueTag::Map)?; if let HeapObject::Map(ref mut map) = self.gc.get_mut(h) { map.remove(&key); } }
            MapContains => { let key = self.pop()?; let h = self.pop_gc(ValueTag::Map)?; let found = match self.gc.get(h) { HeapObject::Map(map) => map.contains_key(&key), _ => false, }; self.stack.push(UValue::Bool(found)); }
            MapKeys => { let h = self.pop_gc(ValueTag::Map)?; let keys = match self.gc.get(h) { HeapObject::Map(map) => map.keys().cloned().collect(), _ => vec![], }; self.stack.push(UValue::Gc(self.gc.alloc(HeapObject::Array(keys)), ValueTag::Array)); }
            MapLen => { let h = self.pop_gc(ValueTag::Map)?; let len = match self.gc.get(h) { HeapObject::Map(map) => map.len(), _ => 0, }; self.stack.push(UValue::Int64(len as i64)); }
            NewSet => { self.stack.push(UValue::Gc(self.gc.alloc(HeapObject::Set(HashSet::new())), ValueTag::Set)); }
            SetAdd => { let val = self.pop()?; let child_h = if let UValue::Gc(ch, _) = &val { Some(*ch) } else { None }; let h = self.pop_gc(ValueTag::Set)?; if let HeapObject::Set(ref mut set) = self.gc.get_mut(h) { set.insert(val); } if let Some(ch) = child_h { self.gc.write_barrier(h, ch); } }
            SetRemove => { let val = self.pop()?; let h = self.pop_gc(ValueTag::Set)?; if let HeapObject::Set(ref mut set) = self.gc.get_mut(h) { set.remove(&val); } }
            SetContains => { let val = self.pop()?; let h = self.pop_gc(ValueTag::Set)?; let found = match self.gc.get(h) { HeapObject::Set(set) => set.contains(&val), _ => false, }; self.stack.push(UValue::Bool(found)); }
            SetLen => { let h = self.pop_gc(ValueTag::Set)?; let len = match self.gc.get(h) { HeapObject::Set(set) => set.len(), _ => 0, }; self.stack.push(UValue::Int64(len as i64)); }
            SetUnion => { let b = self.pop()?; let h = self.pop_gc(ValueTag::Set)?; let result = match self.gc.get(h) { HeapObject::Set(set) => { let mut r = set.clone(); if let UValue::Gc(hb, _) = &b { if let HeapObject::Set(sb) = self.gc.get(*hb) { r.extend(sb.iter().cloned()); } } r } _ => HashSet::new(), }; self.stack.push(UValue::Gc(self.gc.alloc(HeapObject::Set(result)), ValueTag::Set)); }
            SetIntersect => { let b = self.pop()?; let h = self.pop_gc(ValueTag::Set)?; let result = match self.gc.get(h) { HeapObject::Set(set) => { let right: HashSet<UValue> = match &b { UValue::Gc(hb, _) => match self.gc.get(*hb) { HeapObject::Set(sb) => sb.clone(), _ => HashSet::new(), }, _ => HashSet::new(), }; set.intersection(&right).cloned().collect() } _ => HashSet::new(), }; self.stack.push(UValue::Gc(self.gc.alloc(HeapObject::Set(result)), ValueTag::Set)); }
            NewRange => { let step = self.pop()?; let end = self.pop()?; let start = self.pop()?; let range = HeapObject::Range { start: Box::new(start), end: Box::new(end), step: Box::new(step), exclusive: false, }; self.stack.push(UValue::Gc(self.gc.alloc(range), ValueTag::Range)); }
            Tuple => { let count = operand as usize; let base = self.stack.len().saturating_sub(count); let mut elements = Vec::with_capacity(count); for i in base..self.stack.len() { elements.push(self.stack[i].clone()); } self.stack.truncate(base); self.stack.push(UValue::Gc(self.gc.alloc(HeapObject::Tuple(elements)), ValueTag::Tuple)); }
            StrConcat => { let b = self.pop()?; let a = self.pop()?; let s = format!("{}{}", self.value_to_string(&a), self.value_to_string(&b)); let mid = self.current_module_id(); let sid = self.modules[mid].module.intern(&s); self.stack.push(UValue::String(sid)); }
            StrLen => { let v = self.pop()?; let len = match v { UValue::String(sid) => { let mid = self.current_module_id(); self.modules[mid].module.strings.get(sid as usize).map(|s| s.len()).unwrap_or(0) } UValue::Gc(h, tag) if tag == ValueTag::HeapString => { if let HeapObject::HeapString(s) = self.gc.get(h) { s.len() } else { 0 } } _ => 0, }; self.stack.push(UValue::Int64(len as i64)); }
            StrGet => { let mid = self.current_module_id(); let idx = self.pop_int()?; let v = self.pop()?; match v { UValue::String(sid) => { let s = &self.modules[mid].module.strings[sid as usize]; let ch = s.chars().nth(idx as usize).map(|c| c.to_string()).unwrap_or_default(); let new_sid = self.modules[mid].module.intern(&ch); self.stack.push(UValue::String(new_sid)); } _ => self.stack.push(UValue::Nil), } }
            StrSub => { let mid = self.current_module_id(); let end = self.pop_int()?; let start = self.pop_int()?; let v = self.pop()?; match v { UValue::String(sid) => { let s = &self.modules[mid].module.strings[sid as usize]; let s_start = start.max(0) as usize; let s_end = (end as usize).min(s.len()); let sub: String = s.chars().skip(s_start).take(s_end.saturating_sub(s_start)).collect(); let new_sid = self.modules[mid].module.intern(&sub); self.stack.push(UValue::String(new_sid)); } _ => self.stack.push(UValue::Nil), } }
            StrContains => { let mid = self.current_module_id(); let pat = self.pop()?; let v = self.pop()?; let found = match v { UValue::String(sid) => { self.modules[mid].module.strings.get(sid as usize).map(|s| s.contains(&self.value_to_string(&pat))).unwrap_or(false) } _ => false, }; self.stack.push(UValue::Bool(found)); }
            StrIndexOf => { let mid = self.current_module_id(); let pat_val = self.pop()?; let v = self.pop()?; let idx = match v { UValue::String(sid) => { let s = &self.modules[mid].module.strings[sid as usize].clone(); let pat = self.value_to_string(&pat_val); s.find(&pat).map(|i| i as i64).unwrap_or(-1) } _ => -1, }; self.stack.push(UValue::Int64(idx)); }
            StrReplace => { let mid = self.current_module_id(); let replacement = self.pop()?; let pattern = self.pop()?; let v = self.pop()?; match v { UValue::String(sid) => { let s = self.modules[mid].module.strings[sid as usize].clone(); let pat = self.value_to_string(&pattern); let repl = self.value_to_string(&replacement); let result = s.replace(&pat, &repl); let new_sid = self.modules[mid].module.intern(&result); self.stack.push(UValue::String(new_sid)); } _ => self.stack.push(UValue::Nil), } }
            StrSplit => { let mid = self.current_module_id(); let delim = self.pop()?; let v = self.pop()?; match v { UValue::String(sid) => { let s = self.modules[mid].module.strings[sid as usize].clone(); let d = self.value_to_string(&delim); let parts: Vec<UValue> = s.split(&d).map(|p| UValue::String(self.modules[mid].module.intern(p))).collect(); self.stack.push(UValue::Gc(self.gc.alloc(HeapObject::Array(parts)), ValueTag::Array)); } _ => self.stack.push(UValue::Nil), } }
            StrJoin => { let sep = self.pop()?; let arr_val = self.pop()?; let mid = self.current_module_id(); let sep_str = self.value_to_string(&sep); let parts = match arr_val { UValue::Gc(h, _) => match self.gc.get(h) { HeapObject::Array(arr) => arr.iter().map(|v| self.value_to_string(v)).collect::<Vec<_>>(), _ => vec![], }, _ => vec![], }; let result = parts.join(&sep_str); let sid = self.modules[mid].module.intern(&result); self.stack.push(UValue::String(sid)); }
            StrToUpper => { let mid = self.current_module_id(); let v = self.pop()?; match v { UValue::String(sid) => { let s = &self.modules[mid].module.strings[sid as usize]; let upper = s.to_uppercase(); let new_sid = self.modules[mid].module.intern(&upper); self.stack.push(UValue::String(new_sid)); } _ => self.stack.push(v), } }
            StrToLower => { let mid = self.current_module_id(); let v = self.pop()?; match v { UValue::String(sid) => { let s = &self.modules[mid].module.strings[sid as usize]; let lower = s.to_lowercase(); let new_sid = self.modules[mid].module.intern(&lower); self.stack.push(UValue::String(new_sid)); } _ => self.stack.push(v), } }
            StrTrim => { let mid = self.current_module_id(); let v = self.pop()?; match v { UValue::String(sid) => { let s = self.modules[mid].module.strings[sid as usize].clone(); let trimmed = s.trim().to_string(); let new_sid = self.modules[mid].module.intern(&trimmed); self.stack.push(UValue::String(new_sid)); } _ => self.stack.push(v), } }
            StrCmp => { let b = self.pop()?; let a = self.pop()?; let a_str = self.value_to_string(&a); let b_str = self.value_to_string(&b); self.stack.push(UValue::Int32(a_str.cmp(&b_str) as i32)); }
            StrFormat => { let v = self.pop()?; let fmt_val = self.pop()?; let fmt = self.value_to_string(&fmt_val); let val = self.value_to_string(&v); let result = fmt.replacen("%s", &val, 1).replacen("%d", &val, 1).replacen("%f", &val, 1); let mid = self.current_module_id(); let sid = self.modules[mid].module.intern(&result); self.stack.push(UValue::String(sid)); }
            // ── Regex operations ──
            RegexCompile => {
                let s = self.pop()?;
                let pattern = self.value_to_string(&s);
                match regex::Regex::new(&pattern) {
                    Ok(re) => {
                        let compiled = re.as_str().as_bytes().to_vec().into_boxed_slice();
                        let regex_obj = HeapObject::Regex(pattern, compiled);
                        self.stack.push(UValue::Gc(self.gc.alloc(regex_obj), ValueTag::Regex));
                    }
                    Err(e) => {
                        eprintln!("Regex compile error: {e}");
                        self.stack.push(UValue::Nil);
                    }
                }
            }
            RegexMatch => {
                let pat_val = self.pop()?;
                let str_val = self.pop()?;
                let text = self.value_to_string(&str_val);
                let matched = match pat_val {
                    UValue::Gc(h, ValueTag::Regex) => {
                        if let HeapObject::Regex(_, ref compiled) = self.gc.get(h) {
                            let pat_str = std::str::from_utf8(compiled).unwrap_or("");
                            match regex::Regex::new(pat_str) {
                                Ok(re) => re.is_match(&text),
                                Err(_) => false,
                            }
                        } else { false }
                    }
                    // If given a string, compile on-the-fly
                    UValue::String(sid) => {
                        let mid = self.current_module_id();
                        let pat = self.modules[mid].module.strings.get(sid as usize)
                            .cloned().unwrap_or_default();
                        regex::Regex::new(&pat).map(|r| r.is_match(&text)).unwrap_or(false)
                    }
                    _ => false,
                };
                self.stack.push(UValue::Bool(matched));
            }

            // ── OOP / Proto ──
            NewNamespace => {
                let nid = operand;
                let mid = self.current_module_id() as u16;
                let ns = HeapObject::Namespace { name: nid, members: Vec::new(), module_id: mid };
                self.stack.push(UValue::Gc(self.gc.alloc(ns), ValueTag::Namespace));
            }
            NewClass => {
                let ns_handle = self.pop_gc(ValueTag::Namespace)?;
                let class_obj = HeapObject::Class {
                    name: 0,
                    fields: Vec::new(),
                    methods: Vec::new(),
                    parent: None,
                    constructor: None,
                };
                let class_handle = { self.gc.alloc(class_obj) };
                let mid = self.current_module_id();
                let anon_name = { self.modules[mid].module.intern("anon_class") };
                if let HeapObject::Namespace { ref mut members, .. } = self.gc.get_mut(ns_handle) {
                    members.push((anon_name, UValue::Gc(class_handle, ValueTag::Class)));
                }
                self.stack.push(UValue::Gc(class_handle, ValueTag::Class));
            }
            NewObject => {
                let class_handle = self.pop_gc(ValueTag::Class)?;
                let n_fields = if let HeapObject::Class { fields, .. } = self.gc.get(class_handle) { fields.len() } else { 0 };
                let obj = HeapObject::Object { class_handle, fields: vec![UValue::Nil; n_fields], proto: None };
                self.stack.push(UValue::Gc(self.gc.alloc(obj), ValueTag::Object));
            }
            InitStruct => {
                let sid = operand;
                let sd = self.modules[self.current_module_id()].module.get_struct(sid);
                if let Some(sd) = sd {
                    if (sd.size as usize) <= utencore_types::MAX_INLINE_STRUCT_SIZE {
                        self.stack.push(UValue::StructInline(sid, [0u8; MAX_INLINE_STRUCT_SIZE]));
                    } else {
                        let bytes = HeapObject::BoxedStructBytes(vec![0u8; sd.size as usize]);
                        self.stack.push(UValue::Gc(self.gc.alloc(bytes), ValueTag::BoxedStruct));
                    }
                } else { self.stack.push(UValue::Nil); }
            }
            GetField => {
                let fid = operand;
                let v = self.pop()?;
                match v {
                    UValue::StructInline(_, ref bytes) => {
                        let sd = self.modules[self.current_module_id()].module.get_struct(0);
                        if let Some(sd) = sd {
                            if (fid as usize) < sd.fields.len() {
                                let field = &sd.fields[fid as usize];
                                    let val = Self::read_field_from_bytes(bytes, &field.type_ref, 0, &self.modules).unwrap_or(UValue::Nil);
                                    self.stack.push(val);
                            } else { self.stack.push(UValue::Nil); }
                        } else { self.stack.push(UValue::Nil); }
                    }
                    UValue::Gc(h, ValueTag::BoxedStruct) => {
                        if let HeapObject::BoxedStructBytes(ref bytes) = self.gc.get(h) {
                            let sd = self.modules[self.current_module_id()].module.get_struct(0);
                            if let Some(sd) = sd {
                                if (fid as usize) < sd.fields.len() {
                                    let field = &sd.fields[fid as usize];
                                    let val = Self::read_field_from_bytes(bytes, &field.type_ref, 0, &self.modules).unwrap_or(UValue::Nil);
                                    self.stack.push(val);
                                } else { self.stack.push(UValue::Nil); }
                            } else { self.stack.push(UValue::Nil); }
                        } else { self.stack.push(UValue::Nil); }
                    }
                    UValue::Gc(h, ValueTag::Struct) => {
                        if let HeapObject::Struct(fields) = self.gc.get(h) {
                            let val = fields.iter().find(|(id, _)| *id == fid).map(|(_, v)| v.clone()).unwrap_or(UValue::Nil);
                            self.stack.push(val);
                        } else { self.stack.push(UValue::Nil); }
                    }
                    // Namespace member access — caller StringId + ns module_id for string lookup
                    UValue::Gc(h, ValueTag::Namespace) => {
                        let (ns_members, ns_mod_id) = match self.gc.get(h) {
                            HeapObject::Namespace { members, module_id, .. } => (members.clone(), *module_id),
                            _ => (vec![], 0),
                        };
                        let caller_mid = self.current_module_id();
                        // fid comes from the caller's bytecode → use caller's string pool
                        let field_name = self.modules.get(caller_mid)
                            .and_then(|m| m.module.strings.get(fid as usize).cloned())
                            .unwrap_or_default();
                        if field_name.is_empty() { self.stack.push(UValue::Nil); }
                        else {
                            // member sids come from the namespace's module → use ns_mod_id's pool
                            let found = ns_members.iter().find(|(sid, _)| {
                                self.modules.get(ns_mod_id as usize)
                                    .and_then(|m| m.module.strings.get(*sid as usize))
                                    .map(|s| s == &field_name)
                                    .unwrap_or(false)
                            });
                            self.stack.push(found.map(|(_, v)| v.clone()).unwrap_or(UValue::Nil));
                        }
                    }
                    // Object: search fields then class methods
                    UValue::Gc(h, ValueTag::Object) => {
                        let mid = self.current_module_id();
                        let field_name = self.resolve_string_across_modules(fid);
                        if field_name.is_empty() { self.stack.push(UValue::Nil); return Ok(()); }
                        let (fields_data, methods_data) = {
                            let obj = self.gc.get(h);
                            match obj {
                                HeapObject::Object { class_handle, fields, .. } => {
                                    let class_fields = match self.gc.get(*class_handle) {
                                        HeapObject::Class { fields: cf, .. } => cf.clone(),
                                        _ => vec![],
                                    };
                                    let class_methods = match self.gc.get(*class_handle) {
                                        HeapObject::Class { methods, .. } => methods.clone(),
                                        _ => vec![],
                                    };
                                    (fields.clone(), class_methods)
                                }
                                _ => (vec![], vec![]),
                            }
                        };
                        // Check fields first — match by StringId
                        if !fields_data.is_empty() {
                            let fields_info = {
                                let obj = self.gc.get(h);
                                match obj {
                                    HeapObject::Object { class_handle, .. } => {
                                        match self.gc.get(*class_handle) {
                                            HeapObject::Class { fields: cf, .. } => cf.clone(),
                                            _ => vec![],
                                        }
                                    }
                                    _ => vec![],
                                }
                            };
                            if let Some(pos) = fields_info.iter().position(|fsid| *fsid == fid as StringId) {
                                if pos < fields_data.len() {
                                    self.stack.push(fields_data[pos].clone());
                                    return Ok(());
                                }
                            }
                        }
                        // Then class methods
                        for (msid, fr) in &methods_data {
                            if self.resolve_string_across_modules(*msid) == field_name {
                                let closure = HeapObject::Closure {
                                    func: *fr, captures: vec![], module_id: mid as ModuleId,
                                };
                                self.stack.push(UValue::Gc(self.gc.alloc(closure), ValueTag::Closure));
                                return Ok(());
                            }
                        }
                        self.stack.push(UValue::Nil);
                    }
                    // Class method access
                    UValue::Gc(h, ValueTag::Class) => {
                        let mid = self.current_module_id();
                        let field_name = self.resolve_string_across_modules(fid);
                        if field_name.is_empty() { self.stack.push(UValue::Nil); }
                        else {
                            let methods = if let HeapObject::Class { methods, .. } = self.gc.get(h) { methods.clone() }
                                else { vec![] };
                            let found = methods.iter().find(|(msid, _)|
                                self.resolve_string_across_modules(*msid) == field_name
                            );
                            if let Some((_, fr)) = found {
                                let closure = HeapObject::Closure {
                                    func: *fr, captures: vec![], module_id: mid as ModuleId,
                                };
                                self.stack.push(UValue::Gc(self.gc.alloc(closure), ValueTag::Closure));
                            } else { self.stack.push(UValue::Nil); }
                        }
                    }
                    _ => self.stack.push(UValue::Nil),
                }
            }
            SetField => {
                let fid = operand;
                let val = self.pop()?;
                let mut v = self.pop()?; // mut needed for StructInline ref mut bytes
                match v {
                    UValue::StructInline(_, ref mut bytes) => {
                        let sd = self.modules[self.current_module_id()].module.get_struct(0);
                        if let Some(sd) = sd {
                            if (fid as usize) < sd.fields.len() {
                                let field = &sd.fields[fid as usize];
                                Self::write_field_to_bytes(bytes, &field.type_ref, &val);
                            }
                        }
                    }
                    UValue::Gc(h, ValueTag::BoxedStruct) => {
                        let sd = self.modules[self.current_module_id()].module.get_struct(0).cloned();
                        if let HeapObject::BoxedStructBytes(ref mut bytes) = self.gc.get_mut(h) {
                            if let Some(ref sd) = sd {
                                if (fid as usize) < sd.fields.len() {
                                    let field = &sd.fields[fid as usize];
                                    Self::write_field_to_bytes(bytes, &field.type_ref, &val);
                                }
                            }
                        }
                    }
                    UValue::Gc(h, ValueTag::Struct) => {
                        let child_h = if let UValue::Gc(ch, _) = &val { Some(*ch) } else { None };
                        if let HeapObject::Struct(ref mut fields) = self.gc.get_mut(h) {
                            if let Some((_, ref mut fv)) = fields.iter_mut().find(|(id, _)| *id == fid) {
                                *fv = val;
                            } else { fields.push((fid, val)); }
                        }
                        if let Some(ch) = child_h { self.gc.write_barrier(h, ch); }
                    }
                    _ => {}
                }
            }
            // Monomorphize was a catch-all (not an Opcode variant).
            // Generic struct monomorphization will be added as a proper
            // opcode in a future bytecode version.
            GetAttr => {
                let target_sid = operand as StringId;
                let obj = self.pop()?;
                let val = match obj {
                    UValue::Gc(h, ValueTag::Object) => {
                        let mut result = UValue::Nil;

                        // Extract all data from gc BEFORE any gc.alloc
                        let (class_fields, proto_chain, found_method_data) = {
                            let obj_gc = self.gc.get(h);
                            match obj_gc {
                                HeapObject::Object { class_handle, fields, proto, .. } => {
                                    let cf = match self.gc.get(*class_handle) {
                                        HeapObject::Class { fields: cff, .. } => cff.clone(),
                                        _ => vec![],
                                    };
                                    // Check own fields
                                    for (field_sid, _) in cf.iter().enumerate() {
                                        if field_sid as StringId == target_sid {
                                            result = fields[field_sid].clone();
                                            break;
                                        }
                                    }
                                    // Extract proto for chain walk
                                    (cf.clone(), *proto, {
                                        // Check class methods
                                        if matches!(result, UValue::Nil) {
                                            match self.gc.get(*class_handle) {
                                                HeapObject::Class { methods, .. } => {
                                                    methods.iter().find(|(sid, _)| *sid == target_sid).map(|(_, fr)| *fr)
                                                }
                                                _ => None,
                                            }
                                        } else { None }
                                    })
                                }
                                _ => (vec![], None, None),
                            }
                        };

                        // Allocate closure if a method was found (after gc immutable borrow ends)
                        if matches!(result, UValue::Nil) {
                            if let Some(fr) = found_method_data {
                                let mid = self.current_module_id() as ModuleId;
                                let closure = HeapObject::Closure {
                                    func: fr, captures: vec![], module_id: mid,
                                };
                                result = UValue::Gc(self.gc.alloc(closure), ValueTag::Closure);
                            }
                        }

                        // Walk proto chain
                        if matches!(result, UValue::Nil) {
                            let mut current_proto = proto_chain;
                            while let Some(ph) = current_proto {
                                let (pf_clone, pch, next_proto, proto_method) = {
                                    let obj_gc = self.gc.get(ph);
                                    match obj_gc {
                                        HeapObject::Object { fields, class_handle, proto: pp, .. } => {
                                            for (field_sid, _) in class_fields.iter().enumerate() {
                                                if field_sid as StringId == target_sid {
                                                    if field_sid < fields.len() {
                                                        result = fields[field_sid].clone();
                                                    }
                                                    break;
                                                }
                                            }
                                            let method = if matches!(result, UValue::Nil) {
                                                match self.gc.get(*class_handle) {
                                                    HeapObject::Class { methods, .. } => {
                                                        methods.iter().find(|(sid, _)| *sid == target_sid).map(|(_, fr)| *fr)
                                                    }
                                                    _ => None,
                                                }
                                            } else { None };
                                            (fields.clone(), *class_handle, *pp, method)
                                        }
                                        _ => (vec![], 0, None, None),
                                    }
                                };
                                if matches!(result, UValue::Nil) {
                                    if let Some(fr) = proto_method {
                                        let mid = self.current_module_id() as ModuleId;
                                        let closure = HeapObject::Closure {
                                            func: fr, captures: vec![], module_id: mid,
                                        };
                                        result = UValue::Gc(self.gc.alloc(closure), ValueTag::Closure);
                                    }
                                }
                                if matches!(result, UValue::Nil) {
                                    current_proto = next_proto;
                                } else { break; }
                            }
                        }
                        result
                    }
                    UValue::Gc(h, ValueTag::Namespace) => {
                        if let HeapObject::Namespace { members, .. } = self.gc.get(h) {
                            members.iter().find(|(sid, _)| *sid == target_sid).map(|(_, v)| v.clone()).unwrap_or(UValue::Nil)
                        } else { UValue::Nil }
                    }
                    UValue::Gc(h, _) => {
                        UValue::Nil
                    }
                    _ => UValue::Nil,
                };
                self.stack.push(val);
            }
            SetAttr => {
                let target_sid = operand as StringId;
                let val = self.pop()?;
                let val_h = if let UValue::Gc(ch, _) = &val { Some(*ch) } else { None };
                let obj = self.pop()?;
                match obj {
                    UValue::Gc(h, ValueTag::Object) => {
                        // Clone class fields before gc.get_mut to avoid borrow conflict
                        let class_fields = {
                            let obj_gc = self.gc.get(h);
                            if let HeapObject::Object { class_handle, .. } = obj_gc {
                                if let HeapObject::Class { fields: cf, .. } = self.gc.get(*class_handle) {
                                    cf.clone()
                                } else { vec![] }
                            } else { vec![] }
                        };
                        if let HeapObject::Object { fields, .. } = self.gc.get_mut(h) {
                            if let Some(pos) = class_fields.iter().position(|fsid| *fsid == target_sid) {
                                if pos < fields.len() {
                                    fields[pos] = val;
                                }
                            }
                        }
                        if let Some(ch) = val_h { self.gc.write_barrier(h, ch); }
                    }
                    UValue::Gc(h, ValueTag::Namespace) => {
                        if let HeapObject::Namespace { ref mut members, .. } = self.gc.get_mut(h) {
                            if let Some((_, ref mut v)) = members.iter_mut().find(|(sid, _)| *sid == target_sid) {
                                *v = val;
                            } else { members.push((target_sid, val)); }
                        }
                        if let Some(ch) = val_h { self.gc.write_barrier(h, ch); }
                    }
                    _ => {}
                }
            }
            HasAttr => {
                let target_sid = operand as StringId;
                let obj = self.pop()?;
                let found = match obj {
                    UValue::Gc(h, ValueTag::Object) => {
                        let mut result = false;
                        if let HeapObject::Object { class_handle, fields, proto, .. } = self.gc.get(h) {
                            let class_fields = match self.gc.get(*class_handle) {
                                HeapObject::Class { fields: cf, .. } => cf.clone(),
                                _ => vec![],
                            };
                            for (field_sid, _) in class_fields.iter().enumerate() {
                                if field_sid as StringId == target_sid && field_sid < fields.len() && !matches!(fields[field_sid], UValue::Nil) {
                                    result = true;
                                    break;
                                }
                            }
                            if !result {
                                if let HeapObject::Class { methods, .. } = self.gc.get(*class_handle) {
                                    result = methods.iter().any(|(sid, _)| *sid == target_sid);
                                }
                            }
                            if !result {
                                let mut current_proto = *proto;
                                while let Some(ph) = current_proto {
                                    if let HeapObject::Object { fields: pf, class_handle: pch, proto: pp, .. } = self.gc.get(ph) {
                                        let pclass_fields = match self.gc.get(*pch) {
                                            HeapObject::Class { fields: cf, .. } => cf.clone(),
                                            _ => vec![],
                                        };
                                        for (field_sid, _) in pclass_fields.iter().enumerate() {
                                            if field_sid as StringId == target_sid && field_sid < pf.len() && !matches!(pf[field_sid], UValue::Nil) {
                                                result = true;
                                                break;
                                            }
                                        }
                                        if !result {
                                            if let HeapObject::Class { methods, .. } = self.gc.get(*pch) {
                                                result = methods.iter().any(|(sid, _)| *sid == target_sid);
                                            }
                                        }
                                        if !result { current_proto = *pp; } else { break; }
                                    } else { break; }
                                }
                            }
                        }
                        result
                    }
                    _ => false,
                };
                self.stack.push(UValue::Bool(found));
            }
            ClassAddField => {
                let name_sid = operand;
                let class_handle = self.pop_gc(ValueTag::Class)?;
                if let HeapObject::Class { ref mut fields, .. } = self.gc.get_mut(class_handle) {
                    fields.push(name_sid);
                }
                self.stack.push(UValue::Gc(class_handle, ValueTag::Class));
            }
            ClassAddMethod => {
                // Bit 15 of operand: constructor flag (set by compiler)
                // Bits 0-14: method name StringId
                let is_constructor = (operand & 0x8000) != 0;
                let name_sid = (operand & 0x7FFF) as u16;
                let func_val = self.pop()?;
                let func_ref = match func_val {
                    UValue::Int32(i) => i as FuncRef,
                    UValue::Int64(i) => i as FuncRef,
                    UValue::Gc(h, tag) if tag == ValueTag::Closure || tag == ValueTag::Lambda => {
                        if let HeapObject::Closure { func, .. } = self.gc.get(h) { *func }
                        else if let HeapObject::Lambda { func, .. } = self.gc.get(h) { *func }
                        else { return Err(UtenError::TypeError { expected: "function".into(), actual: format!("{:?}", tag) }); }
                    }
                    _ => return Err(UtenError::TypeError { expected: "function ref or closure".into(), actual: format!("{:?}", func_val.tag()) }),
                };
                let class_handle = self.pop_gc(ValueTag::Class)?;
                if let HeapObject::Class { ref mut methods, ref mut constructor, .. } = self.gc.get_mut(class_handle) {
                    methods.push((name_sid as StringId, func_ref));
                    // Constructor flag set by compiler (bit 15 of operand).
                    // No magic name lookup — the compiler EXPLICITLY marks which
                    // method is the constructor. py2uc uses __init__, ts2uc would
                    // use constructor, Lua would use nothing — all work without
                    // the VM knowing language-specific naming conventions.
                    if is_constructor {
                        *constructor = Some(func_ref);
                    }
                }
                self.stack.push(UValue::Gc(class_handle, ValueTag::Class));
            }
            ClassSetParent => {
                let flag = operand;
                let parent_val = self.pop()?;
                let target_val = self.pop()?;
                let parent_handle = match parent_val {
                    UValue::Gc(h, ValueTag::Class) | UValue::Gc(h, ValueTag::Object) => Some(h),
                    _ => None,
                };
                match target_val {
                    UValue::Gc(h, ValueTag::Class) => {
                        if let HeapObject::Class { ref mut parent, .. } = self.gc.get_mut(h) {
                            *parent = parent_handle;
                        }
                    }
                    UValue::Gc(h, ValueTag::Object) => {
                        if let HeapObject::Object { ref mut proto, .. } = self.gc.get_mut(h) {
                            *proto = parent_handle;
                        }
                    }
                    _ => return Err(UtenError::TypeError {
                        expected: "Class or Object",
                        actual: format!("{:?}", target_val.tag()),
                    }),
                }
            }
            // ── OOP: Instance checking and field access ──
            InstanceOf => {
                let class_val = self.pop()?;
                let obj_val = self.pop()?;
                let is_instance = match (&obj_val, &class_val) {
                    (UValue::Gc(oh, ValueTag::Object), UValue::Gc(ch, ValueTag::Class)) => {
                        let mut current = *oh;
                        loop {
                            match self.gc.get(current) {
                                HeapObject::Object { class_handle, proto, .. } => {
                                    if *class_handle == *ch { break true; }
                                    match proto {
                                        Some(p) => current = *p,
                                        None => break false,
                                    }
                                }
                                _ => break false,
                            }
                        }
                    }
                    _ => false,
                };
                self.stack.push(UValue::Bool(is_instance));
            }
            HasField => {
                let name_sid = operand as StringId;
                let obj = self.pop()?;
                let found = match obj {
                    UValue::StructInline(sid, ref bytes) => {
                        let sd = self.modules[self.current_module_id()].module.get_struct(sid);
                        sd.map_or(false, |sd| sd.fields.iter().any(|f| f.name == name_sid))
                    }
                    UValue::Gc(h, ValueTag::Struct) => {
                        match self.gc.get(h) {
                            HeapObject::Struct(fields) => fields.iter().any(|(id, _)| *id == name_sid),
                            _ => false,
                        }
                    }
                    UValue::Gc(h, ValueTag::Object) => {
                        match self.gc.get(h) {
                            HeapObject::Object { class_handle, fields, .. } => {
                                let class_fields = match self.gc.get(*class_handle) {
                                    HeapObject::Class { fields: cf, .. } => cf.clone(),
                                    _ => vec![],
                                };
                                class_fields.iter().any(|fsid| *fsid == name_sid)
                                    || fields.iter().any(|f| !matches!(f, UValue::Nil))
                            }
                            _ => false,
                        }
                    }
                    UValue::Gc(h, ValueTag::Namespace) => {
                        match self.gc.get(h) {
                            HeapObject::Namespace { members, .. } => {
                                // Check both the namespace's module and caller module's string pools
                                members.iter().any(|(sid, _)| {
                                    self.resolve_string_across_modules(*sid) == self.resolve_string_across_modules(name_sid)
                                })
                            }
                            _ => false,
                        }
                    }
                    _ => false,
                };
                self.stack.push(UValue::Bool(found));
            }
            GetFieldIdx => {
                let field_idx = operand as usize;
                let val = self.pop()?;
                match val {
                    UValue::StructInline(sid, ref bytes) => {
                        let sd = self.modules[self.current_module_id()].module.get_struct(sid);
                        if let Some(sd) = sd {
                            if field_idx < sd.fields.len() {
                                let val = Self::read_field_from_bytes(bytes, &sd.fields[field_idx].type_ref, 0, &self.modules).unwrap_or(UValue::Nil);
                                self.stack.push(val);
                            } else { self.stack.push(UValue::Nil); }
                        } else { self.stack.push(UValue::Nil); }
                    }
                    UValue::Gc(h, _) => {
                        if let HeapObject::Struct(fields) = self.gc.get(h) {
                            let val = fields.get(field_idx).map(|(_, v)| v.clone()).unwrap_or(UValue::Nil);
                            self.stack.push(val);
                        } else { self.stack.push(UValue::Nil); }
                    }
                    _ => self.stack.push(UValue::Nil),
                }
            }
            SetFieldIdx => {
                let field_idx = operand as usize;
                let val_to_set = self.pop()?;
                let mut obj = self.pop()?;
                match &mut obj {
                    UValue::StructInline(_, ref mut bytes) => {
                        let sd = self.modules[self.current_module_id()].module.get_struct(0);
                        if let Some(sd) = sd {
                            if field_idx < sd.fields.len() {
                                Self::write_field_to_bytes(bytes, &sd.fields[field_idx].type_ref, &val_to_set);
                            }
                        }
                    }
                    UValue::Gc(_, _) => {}
                    _ => {}
                }
                // For GC objects, write to field directly
                if let UValue::Gc(h, ValueTag::Struct) = &obj {
                    if let HeapObject::Struct(ref mut fields) = self.gc.get_mut(*h) {
                        if field_idx < fields.len() {
                            fields[field_idx] = (fields[field_idx].0, val_to_set);
                        }
                    }
                }
                self.stack.push(obj);
            }
            // NativeFunc/LoadNative are NOT opcode variants — they were
            // irrefutable patterns that shadowed Print/Halt/CIB/Raise etc.
            // Loading native functions by name is done via the Ns namespace
            // in the utencore built-in module (init_unsafe_module).
            // ── 0xF0–0xFF: GC, Debug, etc. ──
            Print => {
                match self.stack.pop() {
                    Some(v) => { let s = self.value_to_string(&v); println!("{s}"); }
                    None => { println!("(empty stack)"); }
                }
            }
            Halt => {
                self.running = false;
            }
            Trace => {
                eprintln!("Stack ({}): {:?}", self.stack.len(), self.stack);
            }
            GcCollect => {
                let gc_ptr: *mut Box<dyn utencore_gc::GcEngine> = &mut self.gc;
                unsafe { (*gc_ptr).collect(self); }
            }
            // ── GC control ──
            Alloc => {
                // Generic heap allocation: create an empty Array (caller can populate)
                self.stack.push(UValue::Gc(self.gc.alloc(HeapObject::Array(Vec::new())), ValueTag::Array));
            }
            GcPin => {
                let val = self.pop()?;
                if let UValue::Gc(h, _) = val {
                    self.gc.pin(h);
                }
            }
            GcUnpin => {
                let val = self.pop()?;
                if let UValue::Gc(h, _) = val {
                    self.gc.unpin(h);
                }
            }
            GcStats => {
                let stats = self.gc.stats();
                // Push as a Map-like structured value: (total_collections, total_allocations, heap_size)
                self.stack.push(UValue::Int64(stats.total_collections as i64));
            }
            WriteBarrier => {
                let child_val = self.pop()?;
                let container_val = self.pop()?;
                if let (UValue::Gc(target, _), UValue::Gc(child, _)) = (&container_val, &child_val) {
                    self.gc.write_barrier(*target, *child);
                }
            }
            GcSetThreshold => {
                let threshold = self.pop_int()?;
                // Store threshold in a dedicated field or adjust config
                // For now, update the GC interval config
                self.config.gc_interval = threshold.max(1) as u32;
            }
            // ── JIT opcodes (stubs — JIT not yet implemented) ──
            JitCompile => {
                // Mark function as hot (placeholder for future JIT)
                let _func_ref = operand;
                // Just log and continue with interpreted execution
                log::info!("JIT compile requested for function {}", operand);
            }
            JitInvalidate => {
                // Invalidate any cached JIT code
                log::info!("JIT invalidate requested");
            }
            JitStat => {
                // Return JIT statistics (none yet)
                self.stack.push(UValue::Int64(0));
            }
            // ── Debug opcodes ──
            Breakpoint => {
                eprintln!("*** BREAKPOINT at pc={} in func {}", self.pc, self.current_func_ref());
                // In a full debugger, this would suspend execution.
                // For now, pause and let the user inspect via Trace.
                self.running = false;
            }
            Line => {
                // Source line marker — updates current line for debug info.
                // No runtime effect; the line number is in the operand for stack traces.
                // (Line mapping is already stored in module.header.line_map)
            }

            // ── CIB opcodes (0xE0–0xEB) ──
            CibLoad => {
                let mid = self.current_module_id();
                let name_sid = operand as StringId;
                let name = self.modules[mid].module.strings.get(name_sid as usize)
                    .cloned().unwrap_or_default();
                match self.cib.load_library(&name) {
                    Ok(()) => {
                        let name_sid = self.modules[mid].module.intern(&name);
                        self.stack.push(UValue::String(name_sid));
                    }
                    Err(e) => {
                        eprintln!("CIB: failed to load library '{name}': {e}");
                        self.stack.push(UValue::Nil);
                    }
                }
            }
            CibSym => {
                let mid = self.current_module_id();
                let sym_sid = operand as StringId;
                let sym_name = self.modules[mid].module.strings.get(sym_sid as usize)
                    .cloned().unwrap_or_default();
                let _lib_name_val = self.pop()?; // library name (for future use)
                match self.cib.find_symbol(&sym_name) {
                    Some(ptr) => {
                        let mid = self.current_module_id();
                        let opaque = HeapObject::Opaque {
                            type_name: self.modules[mid].module.intern("c_fn_ptr"),
                            data: vec![(ptr as usize).to_le_bytes()].concat(),
                        };
                        self.stack.push(UValue::Gc(self.gc.alloc(opaque), ValueTag::Opaque));
                    }
                    None => {
                        eprintln!("CIB: symbol '{sym_name}' not found");
                        self.stack.push(UValue::Nil);
                    }
                }
            }
            CibCall => {
                // Pop args in reverse, then fn ptr, call via libffi
                let nargs = self.pop_int()?;
                let mut args = Vec::with_capacity(nargs as usize);
                for _ in 0..nargs { args.push(self.pop()?); }
                args.reverse();
                let func_val = self.pop()?;
                match func_val {
                    UValue::NativeFn(ref nh) => {
                        // Legacy NativeFnHandle path — needs type info to call
                        // For now, try to call via CibEngine's legacy call
                        match self.cib.call(nh, &args) {
                            Ok(v) => self.stack.push(v),
                            Err(e) => {
                                eprintln!("CIB call error: {e}");
                                self.stack.push(UValue::Nil);
                            }
                        }
                    }
                    UValue::Gc(h, ValueTag::Opaque) => {
                        // Opaque function pointer from CibSym — use DirectCall
                        if let HeapObject::Opaque { data, .. } = self.gc.get(h) {
                            let fn_ptr = usize::from_le_bytes(
                                data[..8].try_into().unwrap_or([0u8; 8])
                            );
                            if fn_ptr == 0 {
                                self.stack.push(UValue::Nil);
                            } else {
                                // Call with void args and void return (simplified)
                                let ret_type = crate::cib::ffi::ffi_type_for(&crate::cib::marshal::CType::Void);
                                let cif = match crate::cib::ffi::prepare_cif(
                                    crate::cib::ffi::FfiAbi::DefaultAbi,
                                    ret_type,
                                    &[],
                                ) {
                                    Ok(cif) => cif,
                                    Err(e) => {
                                        eprintln!("CIB CIF prepare error: {e}");
                                        self.stack.push(UValue::Nil);
                                        return Ok(());
                                    }
                                };
                                let mut ret_buf = [0u8; 8];
                                let mut empty_args: [*mut std::ffi::c_void; 0] = [];
                                unsafe {
                                    crate::cib::ffi::call(
                                        &cif, fn_ptr,
                                        ret_buf.as_mut_ptr() as *mut std::ffi::c_void,
                                        &mut empty_args,
                                    );
                                }
                                self.stack.push(UValue::Int64(
                                    i64::from_le_bytes(ret_buf)
                                ));
                            }
                        } else { self.stack.push(UValue::Nil); }
                    }
                    _ => {
                        eprintln!("CIB: expected function pointer, got {:?}", func_val.tag());
                        self.stack.push(UValue::Nil);
                    }
                }
            }
            CibFree => {
                let mid = self.current_module_id();
                let name_sid = operand as StringId;
                let name = self.modules[mid].module.strings.get(name_sid as usize)
                    .cloned().unwrap_or_default();
                if let Err(e) = self.cib.unload_library(&name) {
                    eprintln!("CIB: failed to unload '{name}': {e}");
                }
                self.stack.push(UValue::Nil);
            }
            CibWrap => {
                let ptr_val = self.pop_int()?;
                let mid = self.current_module_id();
                let type_sid = operand as StringId;
                let type_name = self.modules[mid].module.strings.get(type_sid as usize)
                    .cloned().unwrap_or_default();
                let opaque = HeapObject::Opaque {
                    type_name: self.modules[mid].module.intern(&type_name),
                    data: vec![ptr_val.to_le_bytes()].concat(),
                };
                self.stack.push(UValue::Gc(self.gc.alloc(opaque), ValueTag::Opaque));
            }
            CibUnwrap => {
                let val = self.pop()?;
                if let UValue::Gc(h, ValueTag::Opaque) = val {
                    if let HeapObject::Opaque { data, .. } = self.gc.get(h) {
                        let ptr = usize::from_le_bytes(data[..8].try_into().unwrap_or([0u8; 8]));
                        self.stack.push(UValue::Int64(ptr as i64));
                    } else { self.stack.push(UValue::Int64(0)); }
                } else { self.stack.push(UValue::Int64(0)); }
            }
            CibStrToC => {
                let s = self.pop()?;
                let c_str = self.value_to_string(&s);
                let opaque = HeapObject::Opaque {
                    type_name: 0,
                    data: c_str.as_bytes().to_vec(),
                };
                self.stack.push(UValue::Gc(self.gc.alloc(opaque), ValueTag::Opaque));
            }
            CibStrFromC => {
                let val = self.pop()?;
                if let UValue::Gc(h, ValueTag::Opaque) = val {
                    if let HeapObject::Opaque { data, .. } = self.gc.get(h) {
                        let s = String::from_utf8_lossy(data);
                        let mid = self.current_module_id();
                        let sid = self.modules[mid].module.intern(&s);
                        self.stack.push(UValue::String(sid));
                    } else { self.stack.push(UValue::Nil); }
                } else { self.stack.push(UValue::Nil); }
            }
            CibSizeOf => {
                let mid = self.current_module_id();
                let type_sid = operand as StringId;
                let _type_name = self.modules[mid].module.strings.get(type_sid as usize)
                    .cloned().unwrap_or_default();
                self.stack.push(UValue::Int64(8i64)); // default pointer size
            }
            CibLoadInterface => {
                let mid = self.current_module_id();
                let name_sid = operand as StringId;
                let name = self.modules[mid].module.strings.get(name_sid as usize)
                    .cloned().unwrap_or_default();
                match self.cib.load_interface_file(&name) {
                    Ok(iface_idx) => self.stack.push(UValue::Int64(iface_idx as i64)),
                    Err(e) => {
                        eprintln!("CIB: failed to load interface '{name}': {e}");
                        self.stack.push(UValue::Int64(-1));
                    }
                }
            }
            CibCallTyped => {
                // Pop: [args..., nargs, func_idx]
                // Push: return value
                let func_idx = self.pop_int()? as usize;
                let nargs = self.pop_int()? as usize;
                let mut args = Vec::with_capacity(nargs);
                for _ in 0..nargs { args.push(self.pop()?); }
                args.reverse();
                let iface_idx = self.pop_int()? as usize;
                match self.cib.call_typed(iface_idx, func_idx, &args) {
                    Ok(val) => self.stack.push(val),
                    Err(e) => {
                        eprintln!("CIB typed call error: {e}");
                        self.stack.push(UValue::Nil);
                    }
                }
            }
            CibStructPack => {
                // Pop: struct_name_sid, nfields, then pairs of (field_name_sid, value)
                // Push: packed bytes as GC Bytes object
                let _struct_name_sid = operand as StringId;
                let val = self.pop()?;
                // For now, just push the input value through as a Bytes wrapper.
                // Full implementation needs struct layout lookup + field marshalling.
                let bytes = self.value_to_string(&val).into_bytes();
                self.stack.push(UValue::Gc(self.gc.alloc(HeapObject::Bytes(bytes)), ValueTag::Bytes));
            }

            Export => {
                // Language-agnostic export: pop a value and register it in the
                // current module's export_values table. ImportFunc/ImportValue
                // look here first, so this is the primary mechanism for making
                // symbols visible to importing modules.
                let val = self.pop()?;
                let mid = self.current_module_id();
                let sid = operand as StringId;
                if let Some(name) = self.modules[mid].module.strings.get(sid as usize).cloned() {
                    if !name.is_empty() {
                        self.modules[mid].export_values.insert(name, val);
                    }
                }
            }
            Import => {
                // Language-agnostic import: resolve a module name, load it,
                // and push a Namespace handle wrapping its exports.
                // This means `import foo; foo.bar()` works via standard
                // GetField on the Namespace — no special module_id handling
                // needed in compilers.
                let mid = self.current_module_id();
                let name_sid = operand as StringId;
                let name = self.modules[mid].module.strings.get(name_sid as usize)
                    .cloned().unwrap_or_default();

                let resolved = self.resolve_ns_alias(&name);

                if let Some(module_id) = self.import_module_by_name(&resolved) {
                    let ns_handle = self.build_module_namespace(module_id, &resolved);
                    self.stack.push(UValue::Gc(ns_handle, ValueTag::Namespace));
                } else {
                    eprintln!("Warning: module '{name}' not found (resolved: '{resolved}')");
                    self.stack.push(UValue::Nil);
                }
            }
            ImportFunc => {
                // Language-agnostic named import: look up a symbol in an
                // imported module. Accepts Namespace handle (from Import
                // opcode) or raw module_id integer (legacy compat).
                let name_sid = operand as StringId;
                let module_id_val = self.pop()?;
                let module_id: Option<usize> = match &module_id_val {
                    // New path: Namespace handle (from Import opcode)
                    UValue::Gc(h, ValueTag::Namespace) => {
                        match self.gc.get(*h) {
                            HeapObject::Namespace { module_id, .. } => Some(*module_id as usize),
                            _ => None,
                        }
                    }
                    // Legacy path: raw module_id integer
                    UValue::Int32(id) => Some(*id as usize),
                    UValue::Int64(id) => Some(*id as usize),
                    _ => None,
                };
                let Some(module_id) = module_id else {
                    self.stack.push(UValue::Nil);
                    return Ok(());
                };
                let mid = self.current_module_id();
                let name = self.modules[mid].module.strings.get(name_sid as usize)
                    .cloned().unwrap_or_default();
                if module_id < self.modules.len() {
                    // 1. Primary: export_values (populated by Export opcode + init sync)
                    if let Some(val) = self.modules[module_id].export_values.get(&name) {
                        self.stack.push(val.clone());
                    }
                    // 2. Bytecode-level export table
                    else if let Some(export) = self.modules[module_id].module.exports.get(&name) {
                        match export {
                            ExportEntry::Function(fr) => {
                                let closure = HeapObject::Closure {
                                    func: *fr,
                                    captures: vec![],
                                    module_id: module_id as ModuleId,
                                };
                                self.stack.push(UValue::Gc(self.gc.alloc(closure), ValueTag::Closure));
                            }
                            ExportEntry::Global(g) => {
                                let v = self.modules[module_id].globals.get(*g as usize)
                                    .cloned().unwrap_or(UValue::Nil);
                                self.stack.push(v);
                            }
                            ExportEntry::Type(_) => {
                                self.stack.push(UValue::Nil);
                            }
                        }
                    }
                    // 3. Fallback: search function table by name
                    //    (for modules that use StoreGlobal without Export opcodes)
                    else {
                        let fi = self.modules[module_id].module.functions.iter()
                            .position(|f| f.name == name);
                        if let Some(fi) = fi {
                            let closure = HeapObject::Closure {
                                func: fi as FuncRef,
                                captures: vec![],
                                module_id: module_id as ModuleId,
                            };
                            self.stack.push(UValue::Gc(self.gc.alloc(closure), ValueTag::Closure));
                        } else {
                            self.stack.push(UValue::Nil);
                        }
                    }
                } else {
                    self.stack.push(UValue::Nil);
                }
            }
            ImportValue => {
                // Language-agnostic value import: look up a non-function
                // export. Accepts Namespace handle or raw module_id.
                let name_sid = operand as StringId;
                let module_id_val = self.pop()?;
                let module_id: Option<usize> = match &module_id_val {
                    UValue::Gc(h, ValueTag::Namespace) => {
                        match self.gc.get(*h) {
                            HeapObject::Namespace { module_id, .. } => Some(*module_id as usize),
                            _ => None,
                        }
                    }
                    UValue::Int32(id) => Some(*id as usize),
                    UValue::Int64(id) => Some(*id as usize),
                    _ => None,
                };
                let Some(module_id) = module_id else {
                    self.stack.push(UValue::Nil);
                    return Ok(());
                };
                let mid = self.current_module_id();
                let name = self.modules[mid].module.strings.get(name_sid as usize)
                    .cloned().unwrap_or_default();
                if module_id < self.modules.len() {
                    let val = self.modules[module_id].export_values.get(&name)
                        .cloned().unwrap_or(UValue::Nil);
                    self.stack.push(val);
                } else {
                    self.stack.push(UValue::Nil);
                }
            }
            Raise => {
                let exc_val = self.pop()?;
                let msg = self.value_to_string(&exc_val);
                self.raise_exception(msg)?;
            }

            _ => return Err(UtenError::UnknownOpcode(op as u8)),
        }
        Ok(())
    }
}
