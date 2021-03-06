use crate::parser::Ast;
use im::{hashset, HashSet};
use std::cmp::max;
use std::collections::HashMap;
use std::iter::FromIterator;
use std::{
    fmt,
    fmt::{Display, Formatter},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VarName {
    pub name: String,
    pub kind: Box<Type>,
}

impl Display for VarName {
    fn fmt(self: &Self, f: &mut Formatter<'_>) -> fmt::Result {
        if *self.kind == Type::NoKind {
            write!(f, "{}", self.name)
        } else {
            write!(f, "({} :: {})", self.name, self.kind)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Constructor(VarName),
    TApply(Box<Type>, Box<Type>),
    Variable(VarName),
    NoKind,
    // The reason this exists is because its definition needs itself to exist
    ArrowKind,
    Scheme {
        variables: Vec<VarName>,
        ty: Box<Type>,
    },
}

impl Type {
    // Kind of all types with a runtime value
    pub fn star() -> Type {
        Type::Constructor(VarName {
            name: "*".to_string(),
            kind: Box::new(Type::NoKind),
        })
    }

    // Chain a bunch of type applications together
    pub fn app_chain(self: Type, args: Vec<Type>) -> Type {
        let mut result = self;

        for arg in args {
            result = result.apply(arg)
        }

        result
    }

    pub fn apply(self: &Type, other: Type) -> Type {
        Type::TApply(Box::new(self.clone()), Box::new(other))
    }

    pub fn from_string(name: String) -> Type {
        Type::Variable(VarName {
            name,
            kind: Box::new(Type::NoKind),
        })
    }

    pub fn to_scheme(self: &Type, variables: Vec<VarName>) -> Type {
        Type::Scheme {
            variables,
            ty: Box::new(self.clone()),
        }
    }

    #[inline]
    pub fn create_lambda(from: Type, to: Type) -> Type {
        Type::ArrowKind.apply(from).apply(to)
    }

    #[inline]
    pub fn constant(name: &str) -> Type {
        Type::Constructor(VarName {
            name: String::from(name),
            kind: Box::new(Type::star()),
        })
    }

    #[inline]
    pub fn number() -> Type {
        Type::constant("Number")
    }

    #[inline]
    pub fn string() -> Type {
        Type::constant("String")
    }

    #[inline]
    pub fn boolean() -> Type {
        Type::constant("Boolean")
    }

    // Returns true if the type has a reference to itself
    pub fn is_recursive(self: &Type, variable: &String) -> bool {
        self.free_variables()
            .iter()
            .find(|name| &name.name == variable)
            .is_some()
    }

    // Checks if this is a polymorphic type
    pub fn is_scheme(self: &Type) -> bool {
        match self {
            Type::Scheme {
                variables: _,
                ty: _,
            } => true,
            _ => false,
        }
    }

    pub fn generalize(self: &Type, context: &TypeContext) -> Type {
        let quantifiers = self
            .clone()
            .free_variables()
            .iter()
            .filter(|variable| !context.environment.contains_key(&variable.name))
            .map(Clone::clone)
            .collect();

        self.to_scheme(quantifiers)
    }

    // Check if a type is a function
    pub fn unwrap_function(self: &Type) -> Option<(Type, Type)> {
        if let Type::TApply(first, to) = self {
            if let Type::TApply(fun, from) = *first.clone() {
                if *fun == Type::ArrowKind {
                    Some((*from, *to.clone()))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    }
}

impl Display for Type {
    fn fmt(self: &Type, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Type::Variable(name) => write!(f, "{}", name.name),
            Type::NoKind => write!(f, "[no kind]"),
            ty if ty.unwrap_function().is_some() => {
                let (from, to) = ty.unwrap_function().unwrap();

                if from.unwrap_function().is_some() {
                    write!(f, "({}) -> {}", from, to)
                } else {
                    write!(f, "{} -> {}", from, to)
                }
            }
            Type::TApply(fun, input) => {
                if let Type::TApply(_, _) = **input {
                    write!(f, "{} ({})", fun, input)
                } else {
                    write!(f, "{} {}", fun, input)
                }
            }
            Type::Constructor(VarName { name, kind: _ }) => write!(f, "{}", name),
            Type::ArrowKind => write!(f, "kind(->)"),
            Type::Scheme { variables, ty } => {
                if variables.len() == 0 {
                    write!(f, "{}", ty)
                } else {
                    write!(
                        f,
                        "forall {}. {}",
                        variables
                            .iter()
                            .map(|var| format!("{}", var))
                            .collect::<Vec<String>>()
                            .join(" "),
                        ty
                    )
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum TypeError {
    UnificationError(Type, Type),
    MatchingError(Type, Type),
    SubstitutionConflict(String, Type, Type),
    NotInScope(String),
    RecursiveType(String, Type),
    // This uses Boxes so I don't have to do some random unwrapping in the unify_many function
    DifferentLengths(Vec<Type>, Vec<Type>),
}

impl Display for TypeError {
    fn fmt(self: &TypeError, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TypeError::UnificationError(t1, t2) => {
                write!(f, "Cannot unify type\n    {}\nwith type\n    {}", t1, t2)
            }
            TypeError::MatchingError(t1, t2) => {
                write!(f, "Cannot match type\n    {}\nwith type\n    {}", t1, t2)
            }
            TypeError::SubstitutionConflict(key,t1, t2) => {
                write!(f, "Conflicting substitutions: \n    {} = {}\nand\n    {0} = {}", key,t1, t2)
            }
            TypeError::NotInScope(name) => write!(f, "Variable {} is not in scope", name),
            TypeError::RecursiveType(name, ty) => write!(
                f,
                "Type \n    {} = {}\ncontains references to itself",
                name, ty
            ),
            TypeError::DifferentLengths(tys1, tys2) => write!(
                f,
                "Cannot match length {} with {} while trying to unify types\n    {:?}\nwith\n    {:?}",
                tys1.len(),
                tys2.len(),
                tys1,
                tys2
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub enum TypeConstraint {
    Match(Type, Type),
    Unify(Type, Type),
}

type TypeResult<T = Type> = Result<T, TypeError>;

type TypeEnv = HashMap<String, Type>;

#[derive(Debug, Clone)]
pub struct TypeContext {
    environment: TypeEnv,
    constraints: Vec<TypeConstraint>,
    next_id: u32,
}

impl TypeContext {
    pub fn new() -> TypeContext {
        TypeContext {
            environment: {
                let mut map = TypeEnv::new();
                map.insert("Type".to_string(), Type::star());
                map
            },
            constraints: Vec::new(),
            next_id: 0,
        }
    }

    // Create a constraint requiring 2 types to be equal
    fn should_unify(self: &mut TypeContext, from: &Type, to: &Type) -> () {
        self.constraints
            .push(TypeConstraint::Unify(from.clone(), to.clone()))
    }

    // Create a constraint requiring 1 type to match another type
    fn should_match(self: &mut TypeContext, from: &Type, to: &Type) -> () {
        self.constraints
            .push(TypeConstraint::Match(from.clone(), to.clone()))
    }

    // Generate a new unique id
    pub fn get_id(self: &mut TypeContext) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        id
    }

    // Generate a new unique type variable
    pub fn fresh(self: &mut TypeContext, kind: Type) -> Type {
        let id = self.get_id();
        let name = format!("t{}", id);
        Type::Variable(VarName {
            kind: Box::new(kind),
            name,
        })
    }

    // Replace all quantifiers with fresh variables
    pub fn instantiate(self: &mut TypeContext, ty: &Type) -> Type {
        match ty {
            Type::Scheme { variables, ty } => {
                let new_variables = variables
                    .into_iter()
                    .map(|var| (var.name.clone(), self.fresh(*var.kind.clone())));
                let substitution = new_variables.collect();

                ty.clone().apply_substitution(&substitution)
            }
            other => other.clone(),
        }
    }

    fn solve_constraints_with_subst(
        self: &mut TypeContext,
        constraints: &Vec<TypeConstraint>,
        substitution: Substitution,
    ) -> TypeResult<Substitution> {
        match &constraints[..] {
            [] => Ok(substitution),
            [constraint, ..] => {
                let new_subst = match constraint {
                    TypeConstraint::Unify(left, right) => {
                        let left = self.kind_unkinded(left.clone());
                        let right = self.kind_unkinded(right.clone());
                        merge_substitutions(self.unify(left, right)?, substitution)
                    }
                    TypeConstraint::Match(left, right) => {
                        let left = self.kind_unkinded(left.clone());
                        let right = self.kind_unkinded(right.clone());
                        merge_substitutions(self.match_types(left, right)?, substitution)
                    }
                };
                let constraints = constraints[1..].to_vec().apply_substitution(&new_subst);
                self.solve_constraints_with_subst(&constraints, new_subst)
            }
        }
    }

    pub fn solve_constraints(self: &mut TypeContext) -> TypeResult<Substitution> {
        let initial_constraints = self.constraints.clone();

        self.constraints = vec![];

        let subst = self.solve_constraints_with_subst(&initial_constraints, Substitution::new())?;

        if self.constraints.len() > 0 {
            println!("Found more constraints, continuing to solve");
            self.constraints = self.constraints.clone().apply_substitution(&subst);
            let subst2 = self.solve_constraints()?;

            Ok(merge_substitutions(subst2, subst))
        } else {
            Ok(subst)
        }
    }

    // Create a new context based on a new variable
    pub fn create_closure(self: &TypeContext, name: String, scheme: Type) -> TypeContext {
        let mut context = self.clone();

        context.environment.insert(name, scheme);

        context
    }

    // Infer a expression inside a closure
    pub fn infer_with(self: &mut TypeContext, name: String, scheme: Type, ast: Ast) -> TypeResult {
        let mut new_ctx = self.create_closure(name, scheme);
        let result = new_ctx.infer(ast);

        self.sync(new_ctx);

        result
    }

    // copy stuff over from another context
    pub fn sync(self: &mut TypeContext, other: TypeContext) -> () {
        self.constraints.extend(other.constraints);
        self.next_id = max(other.next_id, self.next_id);
    }

    // Infer the type of an expression
    pub fn infer(self: &mut TypeContext, expression: Ast) -> TypeResult {
        match expression {
            Ast::FloatLiteral(_) => Ok(Type::number()),
            Ast::StringLiteral(_) => Ok(Type::string()),
            Ast::Annotation(annotated, annotation) => {
                let inferred = self.infer(*annotated)?;

                self.should_match(&inferred, &annotation);

                Ok(inferred)
            }
            Ast::If(condition, right, left) => {
                let type_condition = self.infer(*condition)?;
                let type_right = self.infer(*right)?;
                let type_left = self.infer(*left)?;
                self.should_unify(&type_condition, &Type::boolean());
                self.should_unify(&type_left, &type_right);

                Ok(type_right)
            }
            Ast::Variable(name) => match self.environment.clone().get(&name) {
                Some(result) => Ok(self.instantiate(result)),
                None => Err(TypeError::NotInScope(name)),
            },
            Ast::FunctionCall(function, argument) => {
                let return_type = self.fresh(Type::NoKind);
                let function_type = self.infer(*function)?;
                let argument_type = self.infer(*argument)?;

                self.should_unify(
                    &function_type,
                    &Type::create_lambda(argument_type, return_type.clone()),
                );

                Ok(return_type)
            }
            Ast::Lambda(argument, body) => {
                let arg_type = self.fresh(Type::NoKind);
                let return_type = self.infer_with(argument, arg_type.clone(), *body)?;

                Ok(Type::create_lambda(arg_type, return_type))
            }
            Ast::Let(name, value, body) => {
                let mut value_ctx = self.clone();
                value_ctx.constraints = Vec::new();
                let value_type = value_ctx.infer(*value)?;
                let substitution = value_ctx.solve_constraints()?;

                self.with_substitution(&substitution);

                let scheme = value_type
                    .apply_substitution(&substitution)
                    .generalize(self);

                self.sync(value_ctx);
                self.infer_with(name, scheme, *body)
            }
        }
    }

    // Applies a substitution on the current environment
    pub fn with_substitution(self: &mut Self, substitution: &Substitution) -> () {
        self.environment = self.environment.clone().apply_substitution(substitution);
    }

    pub fn constrain_type_application(self: &mut Self, func: Type, input: Type) -> (Type, Type) {
        let k_ret = self.fresh(Type::NoKind);
        let k_fun = self.get_kind(func);
        let k_input = self.get_kind(input);

        (k_fun, Type::create_lambda(k_input, k_ret.clone()))
    }

    // Matching is like unification but variables can only be bound on 1 side
    pub fn match_types(self: &mut Self, left: Type, right: Type) -> TypeResult<Substitution> {
        match (&left, &right) {
            (left, right) if left == right => Ok(Substitution::new()),
            (Type::NoKind, _) => Ok(Substitution::new()),
            (_, Type::NoKind) => Ok(Substitution::new()),
            (
                Type::Constructor(VarName {
                    name: name_left,
                    kind: kind_left,
                }),
                Type::Constructor(VarName {
                    name: name_right,
                    kind: kind_right,
                }),
            ) if name_right == name_left => {
                self.match_types(*kind_left.clone(), *kind_right.clone())
            }
            (scheme, other) if scheme.is_scheme() => {
                let instantiated = self.instantiate(scheme);
                self.match_types(instantiated, other.clone())
            }
            (other, scheme) if scheme.is_scheme() => {
                let instantiated = self.instantiate(scheme);
                self.match_types(other.clone(), instantiated)
            }
            (Type::Variable(var), right) => {
                self.bind_type_variable(var.name.clone(), Some(*var.kind.clone()), right.clone())
            }
            (Type::TApply(fun_left, input_left), Type::TApply(fun_right, input_right)) => {
                let constraint_left =
                    self.constrain_type_application(*fun_left.clone(), *input_left.clone());
                let constraint_right =
                    self.constrain_type_application(*fun_right.clone(), *input_right.clone());

                let subst2 = self.unify_many(
                    vec![constraint_left.0, constraint_right.0],
                    vec![constraint_left.1, constraint_right.1],
                )?;

                let fun_subst = self.match_types(
                    fun_left.clone().apply_substitution(&subst2),
                    fun_right.clone().apply_substitution(&subst2),
                )?;
                let input_subst = self.match_types(
                    input_left.clone().apply_substitution(&subst2),
                    input_right.clone().apply_substitution(&subst2),
                )?;

                Ok(merge_substitutions(
                    subst2,
                    safe_merge_substitution(fun_subst, input_subst)?,
                ))
            }
            (Type::ArrowKind, Type::ArrowKind) => Ok(Substitution::new()),
            (left, right) => Err(TypeError::MatchingError(left.clone(), right.clone())),
        }
    }

    // UNIFY TYPES
    pub fn unify(self: &mut Self, left: Type, right: Type) -> TypeResult<Substitution> {
        match (&left, &right) {
            (left, right) if left == right => Ok(Substitution::new()),
            (Type::NoKind, _) => Ok(Substitution::new()),
            (_, Type::NoKind) => Ok(Substitution::new()),
            (
                Type::Constructor(VarName {
                    name: name_left,
                    kind: kind_left,
                }),
                Type::Constructor(VarName {
                    name: name_right,
                    kind: kind_right,
                }),
            ) if name_right == name_left => self.unify(*kind_left.clone(), *kind_right.clone()),
            (scheme, other) if scheme.is_scheme() => {
                let instantiated = self.instantiate(scheme);
                self.unify(instantiated, other.clone())
            }
            (other, scheme) if scheme.is_scheme() => {
                let instantiated = self.instantiate(scheme);
                self.unify(instantiated, other.clone())
            }
            (Type::Variable(var), right) => {
                self.bind_type_variable(var.name.clone(), Some(*var.kind.clone()), right.clone())
            }
            (left, Type::Variable(var)) => {
                self.bind_type_variable(var.name.clone(), Some(*var.kind.clone()), left.clone())
            }
            (Type::TApply(fun_left, input_left), Type::TApply(fun_right, input_right)) => {
                let constraint_left =
                    self.constrain_type_application(*fun_left.clone(), *input_left.clone());
                let constraint_right =
                    self.constrain_type_application(*fun_right.clone(), *input_right.clone());

                self.unify_many(
                    vec![
                        constraint_left.0,
                        constraint_right.0,
                        *fun_left.clone(),
                        *input_left.clone(),
                    ],
                    vec![
                        constraint_left.1,
                        constraint_right.1,
                        *fun_right.clone(),
                        *input_right.clone(),
                    ],
                )
            }
            (left, right) => Err(TypeError::UnificationError(left.clone(), right.clone())),
        }
    }

    // Unify 2 vectors of types 1 by 1
    pub fn unify_many(
        self: &mut Self,
        types1: Vec<Type>,
        types2: Vec<Type>,
    ) -> TypeResult<Substitution> {
        match (types1.split_first(), types2.split_first()) {
            (None, None) => Ok(Substitution::new()),
            (Some((left, types1)), Some((right, types2))) => {
                let substitution = self.unify(left.clone(), right.clone())?;
                let other_substitution = self.unify_many(
                    Vec::from(types1).apply_substitution(&substitution),
                    Vec::from(types2).apply_substitution(&substitution),
                )?;
                Ok(merge_substitutions(other_substitution, substitution))
            }
            _ => Err(TypeError::DifferentLengths(
                types1.to_vec(),
                types2.to_vec(),
            )),
        }
    }

    // Get the kind of a type generating constraints on the ways
    pub fn get_kind(self: &mut Self, ty: Type) -> Type {
        match ty {
            scheme if scheme.is_scheme() => {
                let instantiated = self.instantiate(&scheme);

                self.get_kind(instantiated)
            }
            Type::Constructor(VarName { name: _, kind }) => *kind.clone(),
            Type::Variable(VarName { name: _, kind }) => *kind.clone(),
            Type::ArrowKind => {
                let k = Type::NoKind;
                Type::create_lambda(k.clone(), Type::create_lambda(k.clone(), k)).generalize(self)
            }
            Type::TApply(fun, input) => {
                if *fun == Type::ArrowKind {
                    // Type::create_lambda(Type::NoKind, Type::NoKind).generalize(self)
                    Type::NoKind
                } else {
                    let k_ret = self.fresh(Type::NoKind);
                    let k_input = self.get_kind(*input.clone());
                    let k_fun = self.get_kind(*fun.clone());

                    self.should_unify(&k_fun, &Type::create_lambda(k_input, k_ret.clone()));

                    k_ret
                }
            }
            Type::NoKind => Type::NoKind,
            other => panic!("Cannot get kind of type {}", other),
        }
    }

    // Todo: remove
    pub fn kind_unkinded(self: &mut Self, ty: Type) -> Type {
        ty.clone()
    }

    // Bind a type variable to a type
    pub fn bind_type_variable(
        self: &mut Self,
        var_name: String,
        var_kind: Option<Type>,
        ty: Type,
    ) -> TypeResult<Substitution> {
        match &ty {
            Type::Variable(other_var) if *other_var.name == var_name => match var_kind {
                None => Ok(Substitution::new().update(var_name, ty)),
                Some(kind) => self.unify(*other_var.kind.clone(), kind.clone()),
            },
            other => {
                if ty.is_recursive(&var_name) {
                    Err(TypeError::RecursiveType(var_name, ty))
                } else {
                    match var_kind {
                        None => Ok(Substitution::new().update(var_name, ty)),
                        Some(var_kind) => {
                            let k_other = self.get_kind(other.clone());
                            let subst = self.unify(k_other, var_kind.clone())?;
                            let map = subst.update(var_name, ty.apply_substitution(&subst));
                            Ok(map)
                        }
                    }
                }
            }
        }
    }
}

pub type Substitution = im::HashMap<String, Type>;

fn merge_substitutions(subst1: Substitution, subst2: Substitution) -> Substitution {
    subst2.apply_substitution(&subst1).union(subst1)
}

// This merges substitutions without duplicate keys
fn safe_merge_substitution(subst1: Substitution, subst2: Substitution) -> TypeResult<Substitution> {
    for key in subst1.clone().intersection(subst2.clone()).keys() {
        let left = Type::from_string(key.clone()).apply_substitution(&subst1);
        let right = Type::from_string(key.clone()).apply_substitution(&subst2);
        if left != right {
            return Err(TypeError::SubstitutionConflict(key.clone(), left, right));
        }
    }

    Ok(subst1.union(subst2))
}

pub trait Substituable {
    fn free_variables(self: &Self) -> HashSet<VarName>;
    fn apply_substitution(self: Self, substitution: &Substitution) -> Self;
}

impl<T: Substituable + Clone> Substituable for Vec<T> {
    fn free_variables(self: &Self) -> HashSet<VarName> {
        self.iter()
            .flat_map(|ty| ty.clone().free_variables())
            .collect()
    }

    fn apply_substitution(self: Self, substitution: &Substitution) -> Self {
        let mut result = Vec::new();

        for ty in self {
            result.push(ty.clone().apply_substitution(substitution))
        }
        result
    }
}

impl Substituable for TypeConstraint {
    fn free_variables(self: &Self) -> HashSet<VarName> {
        let (left, right) = match self {
            TypeConstraint::Match(left, right) => (left, right),
            TypeConstraint::Unify(left, right) => (left, right),
        };

        left.free_variables().union(right.free_variables())
    }

    fn apply_substitution(self: Self, substitution: &Substitution) -> Self {
        match self {
            TypeConstraint::Match(left, right) => TypeConstraint::Match(
                left.apply_substitution(substitution),
                right.apply_substitution(substitution),
            ),
            TypeConstraint::Unify(left, right) => TypeConstraint::Unify(
                left.apply_substitution(substitution),
                right.apply_substitution(substitution),
            ),
        }
    }
}

impl Substituable for Substitution {
    fn free_variables(self: &Substitution) -> HashSet<VarName> {
        self.values()
            .flat_map(|ty| ty.clone().free_variables())
            .collect()
    }

    fn apply_substitution(self: Self, substitution: &Substitution) -> Substitution {
        self.iter()
            .map(|(key, ty)| (key.clone(), ty.clone().apply_substitution(substitution)))
            .collect()
    }
}

impl Substituable for TypeEnv {
    fn free_variables(self: &Self) -> HashSet<VarName> {
        self.values()
            .flat_map(|ty| ty.clone().free_variables())
            .collect()
    }

    fn apply_substitution(self: Self, substitution: &Substitution) -> TypeEnv {
        HashMap::from_iter(
            self.iter().map(|(key, scheme)| {
                (key.clone(), scheme.clone().apply_substitution(substitution))
            }),
        )
    }
}

impl Substituable for TypeContext {
    fn free_variables(self: &Self) -> HashSet<VarName> {
        self.environment.free_variables()
    }

    fn apply_substitution(self: Self, substitution: &Substitution) -> TypeContext {
        TypeContext {
            environment: self.environment.apply_substitution(substitution),
            ..self
        }
    }
}

impl Substituable for Type {
    fn free_variables(self: &Type) -> HashSet<VarName> {
        match self {
            Type::Variable(name) => hashset![name.clone()],
            Type::TApply(fun, input) => fun.free_variables().union(input.free_variables()),
            Type::Scheme { variables, ty } => {
                let quantifier_names: Vec<_> = variables.iter().map(|v| v.name.clone()).collect();
                ty.free_variables()
                    .iter()
                    .filter(|v| !quantifier_names.contains(&v.name))
                    .map(Clone::clone)
                    .collect()
            }
            _ => HashSet::new(),
        }
    }

    fn apply_substitution(self: Type, substitution: &Substitution) -> Type {
        match &self {
            Type::Variable(VarName { name, kind: _ }) => match substitution.get(name) {
                Some(new_type) => new_type.clone(),
                None => self,
            },
            Type::TApply(fun, input) => (*fun.clone())
                .apply_substitution(substitution)
                .apply(input.clone().apply_substitution(substitution)),
            _ => self,
        }
    }
}

// ACTUAL FUNCTION FOR GETTING THE TYPE OF AN EXPRESSION
pub fn get_type_of(expression: Ast) -> TypeResult {
    let mut context = TypeContext::new();
    let resulting_type = context.infer(expression)?;
    let subst = context.solve_constraints()?;

    Ok(resulting_type
        .apply_substitution(&subst)
        .generalize(&context))
}
