mod node;
mod var;
mod vardiff;

use ndarray::{ArrayViewMutD, Ix, RawArrayViewMut};
use std::{
    cell::{Ref, RefCell},
    collections::{BTreeMap, HashSet},
    hash::{Hash, Hasher},
    rc::Rc,
};
pub use var::Var;
pub use vardiff::VarDiff;

pub(crate) use node::*;
pub use node::{
    Backward, Cache, Constant, Convolve, ConvolveWithGroups, Data, Eval, Forward, Gradient, Input,
    InputBackward, Overwrite, PaddingMode, Reflective, Replicative, Zero,
};

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ Global Var Identifier ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
/// Keeps track of each operations. It is also used to provide an identifier to computational nodes.
pub(crate) struct OperationsCounter {
    count: usize,
}

impl OperationsCounter {
    pub fn next(&mut self) -> usize {
        self.count += 1;
        self.count
    }
}

pub(crate) static mut OPERATIONS_COUNTER: OperationsCounter = OperationsCounter { count: 0 };

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ Histories ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(Clone)]
/// The computational forward-history of a variable. It keeps track of the computation up to the
/// variable to whom the struct belongs.
pub struct VarHistory {
    path: BTreeMap<usize, Rc<dyn Forward>>,
    buffer: RefCell<Vec<Rc<dyn Forward>>>,
    changeables: HashSet<Changeable>,
}

impl VarHistory {
    /// Returns a new, empty, `VarHistory`.
    pub(crate) fn new() -> Self {
        Self {
            path: BTreeMap::new(),
            buffer: RefCell::new(Vec::new()),
            changeables: HashSet::new(),
        }
    }

    /// Merges `self` and `other`. This is equivalent to a set-intersection.
    ///
    /// # Arguments
    ///
    /// `other` - other VarHistory.
    pub(crate) fn merge(&mut self, mut other: VarHistory) {
        self.path.append(&mut other.path);
    }

    /// Appends a new forward computational node to `self`. The new node has id `id`.
    ///
    /// # Arguments
    ///
    /// * `id` - id of the new node.
    /// * `next` - node to append.
    pub(crate) fn append_forward(&mut self, id: usize, next: Rc<dyn Forward>) {
        self.path.insert(id, next);
        self.buffer.borrow_mut().truncate(0);
    }

    /// Appends a new eval computational node to `self`. The new node has id `id`.
    ///
    /// # Arguments
    ///
    /// * `next` - node to append.
    pub(crate) fn append_changeable(&mut self, next: Changeable) {
        self.changeables.insert(next);
    }

    /// Returns the length of the forward path.
    pub(crate) fn len(&self) -> usize {
        self.path.len()
    }

    /// Returns `true` if the forward path contains no node.
    pub(crate) fn is_empty(&self) -> bool {
        self.path.is_empty()
    }

    /// Prepares the buffer. Clones and transfers the content of the forward path
    /// into a vector. Such vector will be used to perform the actual forward pass.
    pub(crate) fn prepare_buffer(&self) {
        if self.buffer.borrow().is_empty() {
            *self.buffer.borrow_mut() = self.path.values().cloned().collect();
        }
    }

    /// Returns a reference to the buffer.
    pub(crate) fn buffer(&self) -> Ref<[Rc<dyn Forward>]> {
        Ref::map(self.buffer.borrow(), |vec| &vec[..])
    }
}

#[derive(Clone)]
/// The computational backward-history of a variable. It keeps track of the computation up to the
/// variable to whom the struct belongs.
pub struct VarDiffHistory {
    path: BTreeMap<usize, Rc<dyn Backward>>,
    buffer: RefCell<Vec<Rc<dyn Backward>>>,
    parameters: HashSet<RawParam>,
}

impl VarDiffHistory {
    /// Returns a new, empty, `VarDiffHistory` with  parameters `parameters`.
    ///
    /// # Arguments
    ///
    /// ` parameters` - parameters to store.
    pub(crate) fn new(parameters: HashSet<RawParam>) -> Self {
        Self {
            path: BTreeMap::new(),
            buffer: RefCell::new(Vec::new()),
            parameters,
        }
    }

    /// Merges `self` and `other`. This is equivalent to a set-intersection.
    ///
    /// # Arguments
    ///
    /// `other` - other VarDiffHistory.
    pub(crate) fn merge(&mut self, mut other: VarDiffHistory) {
        self.path.append(&mut other.path);
        self.parameters.extend(other.parameters);
    }

    /// Appends a new backward computational node to `self`. The new node has id `id`.
    ///
    /// # Arguments
    ///
    /// * `id` - id of the new node.
    /// * `next` - node to append.
    pub(crate) fn append_backward(&mut self, id: usize, next: Rc<dyn Backward>) {
        self.path.insert(id, next);
        self.buffer.borrow_mut().truncate(0);
    }

    /// Returns the length of the backward path.
    pub(crate) fn len(&self) -> usize {
        self.path.len()
    }

    /// Returns `true` if the backward path contains no node.
    pub(crate) fn is_empty(&self) -> bool {
        self.path.is_empty()
    }

    /// Prepares the buffer. Clones and transfers the content of the backward path
    /// into a vector. Such vector will be used to perform the actual backward pass.
    pub(crate) fn prepare_buffer(&self) {
        if self.buffer.borrow().is_empty() {
            *self.buffer.borrow_mut() = self.path.values().cloned().collect();
        }
    }

    /// Returns a reference to the buffer.
    pub(crate) fn buffer(&self) -> Ref<[Rc<dyn Backward>]> {
        Ref::map(self.buffer.borrow(), |vec| &vec[..])
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ RawParam Struct ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

/// A builder of mutable views over a differentiable variable's data and gradient.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct RawParam {
    data: *mut f32,
    grad: *mut f32,
    shape: Vec<Ix>,
}

impl RawParam {
    pub(crate) fn new(data: *mut f32, grad: *mut f32, shape: Vec<Ix>) -> Self {
        Self { data, grad, shape }
    }

    /// Consumes the RawParam, yielding mutable views over the data and the gradient of the
    /// differentiable variable that it refers to. The lifetime `'a` is for the
    /// scope of the borrow.
    pub(crate) fn into_param<'a>(self) -> Param<'a> {
        let shape = self.shape;

        unsafe {
            let raw_data = RawArrayViewMut::from_shape_ptr(shape.clone(), self.data);
            let raw_grad = RawArrayViewMut::from_shape_ptr(shape, self.grad);
            let data = raw_data.deref_into_view_mut();
            let grad = raw_grad.deref_into_view_mut();
            Param { data, grad }
        }
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ Param Struct ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

/// Mutable views over a differentiable variable's data and gradient.
///
/// See also [`.parameters()`] and [`ModelStatus`] for more details.
///
///
/// The views are [`ndarray::ArrayViewMutD`].
///
/// [`ndarray::ArrayViewMutD`]: ndarray::ArrayViewMutD
///
/// [`.parameters()`]: VarDiff::parameters()
/// [`ModelStatus`]: crate::nn::ModelStatus
#[derive(Debug)]
pub struct Param<'a> {
    pub data: ArrayViewMutD<'a, f32>,
    pub grad: ArrayViewMutD<'a, f32>,
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ Changeable struct ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(Clone)]
/// Hashable and comparable wrapper for a computational node that implements the `Eval` trait.
pub(super) struct Changeable {
    id: usize,
    node: Rc<dyn Eval>,
}

impl PartialEq for Changeable {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Changeable {}

impl Hash for Changeable {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ Algebraic Traits ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ Matrix Multiplication ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

/// Matrix-matrix multiplication.
pub trait MatMatMul<Rhs> {
    /// The type of the matrix-matrix multiplication's result. See the
    /// [*differentiability arithmetic*] for more details.
    ///
    /// [*differentiability arithmetic*]: index.html#differentiability-arithmetic
    type Output;

    /// Computes the matrix-matrix multiplication between `self` and `other`.
    fn mm(self, other: Rhs) -> Self::Output;
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~ Matrix Multiplication with Transposition ~~~~~~~~~~~~~~~~~~~~~~~~~~~

/// Matrix-matrix multiplication with transposed right hand side operand.
///
/// This fused operation is marginally faster than performing the matrix-matrix multiplication
/// and transposition separately.
pub trait MatMatMulT<Rhs> {
    /// The type of the matrix-matrix multiplication with transposed right hand side operand's
    /// result. See the [*differentiability arithmetic*] for more details.
    ///
    /// [*differentiability arithmetic*]: index.html#differentiability-arithmetic
    type Output;

    /// Computes the matrix-matrix multiplication between `self` and transposed `other`.
    fn mm_t(self, other: Rhs) -> Self::Output;
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ Matrix Vector Multiplication ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

/// Matrix-vector multiplication.
pub trait MatVecMul<Rhs> {
    /// The type of the matrix-vector multiplication's result. See the
    /// [*differentiability arithmetic*] for more details.
    ///
    /// [*differentiability arithmetic*]: index.html#differentiability-arithmetic
    type Output;

    /// Computes the matrix-vector multiplication between `self` and `other`.
    fn mv(self, other: Rhs) -> Self::Output;
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ Vector Matrix Multiplication ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

/// Vector-matrix multiplication.
pub trait VecMatMul<Rhs> {
    /// The type of the vector-matrix multiplication's result. See the
    /// [*differentiability arithmetic*] for more details.
    ///
    /// [*differentiability arithmetic*]: index.html#differentiability-arithmetic
    type Output;

    /// Computes the vector-matrix multiplication between `self` and `other`.
    fn vm(self, other: Rhs) -> Self::Output;
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ Vector Vector Multiplication ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

/// Vector-vector multiplication, *a.k.a. dot product or inner product*.
pub trait VecVecMul<Rhs> {
    /// The type of the dot product's result. See the [*differentiability arithmetic*] for
    /// more details.
    ///
    /// [*differentiability arithmetic*]: index.html#differentiability-arithmetic
    type Output;

    /// Computes the dot product between `self` and `other`.
    fn vv(self, other: Rhs) -> Self::Output;
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ Cat and Stack traits ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

/// Concatenation.
pub trait Cat<Rhs> {
    /// The type of the concatenation's result. See the [*differentiability arithmetic*] for
    /// more details.
    ///
    /// [*differentiability arithmetic*]: index.html#differentiability-arithmetic
    type Output;

    /// Concatenates variables along the given axis.
    fn cat(self, other: Rhs, axis: usize) -> Self::Output;
}

/// Stacking.
pub trait Stack<Rhs> {
    /// The type of the stacking's result. See the [*differentiability arithmetic*] for
    /// more details.
    ///
    /// [*differentiability arithmetic*]: index.html#differentiability-arithmetic
    type Output;

    /// Stacks variables along the given axis.
    fn stack(self, other: Rhs, axis: usize) -> Self::Output;
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ Tests ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
#[cfg(test)]
mod test;
