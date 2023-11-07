use bimap::{BiHashMap, Overwritten};
use derive_more::{Display, From, Into};
use hvmc::run::Val;
use itertools::Itertools;
use shrinkwraprs::Shrinkwrap;
use std::{
  collections::{BTreeMap, HashMap},
  fmt,
};

pub mod check;
pub mod load_book;
pub mod net_to_term;
pub mod parser;
pub mod term_to_net;
pub mod transform;

pub use net_to_term::net_to_term_linear;
pub use term_to_net::{book_to_nets, term_to_compat_net};

/// The representation of a program.
#[derive(Debug, Clone, Default)]
pub struct Book {
  /// Mapping of definition names to ids.
  pub def_names: DefNames,

  /// The function definitions.
  pub defs: BTreeMap<DefId, Definition>,

  /// The algebraic datatypes defined by the program
  pub adts: HashMap<Name, Adt>,

  /// To which type does each constructor belong to.
  pub ctrs: HashMap<Name, Name>,
}

#[derive(Debug, Clone, Default)]
pub struct DefNames {
  map: BiHashMap<DefId, Name>,
  id_count: DefId,
}

/// A pattern matching function definition.
#[derive(Debug, Clone)]
pub struct Definition {
  pub def_id: DefId,
  pub rules: Vec<Rule>,
}

/// A pattern matching rule of a definition.
#[derive(Debug, Clone)]
pub struct Rule {
  pub def_id: DefId,
  pub pats: Vec<RulePat>,
  pub body: Term,
}

#[derive(Debug, Clone)]
pub enum RulePat {
  Var(Name),
  Ctr(Name, Vec<RulePat>),
}

#[derive(Debug, Clone)]
pub enum Term {
  Lam {
    nam: Option<Name>,
    bod: Box<Term>,
  },
  Var {
    nam: Name,
  },
  /// Like a scopeless lambda, where the variable can occur outside the body
  Chn {
    nam: Name,
    bod: Box<Term>,
  },
  /// The use of a Channel variable.
  Lnk {
    nam: Name,
  },
  Let {
    pat: LetPat,
    val: Box<Term>,
    nxt: Box<Term>,
  },
  Ref {
    def_id: DefId,
  },
  App {
    fun: Box<Term>,
    arg: Box<Term>,
  },
  Match {
    cond: Box<Term>,
    zero: Box<Term>,
    succ: Box<Term>,
  },
  Dup {
    fst: Option<Name>,
    snd: Option<Name>,
    val: Box<Term>,
    nxt: Box<Term>,
  },
  Sup {
    fst: Box<Term>,
    snd: Box<Term>,
  },
  Era,
  Num {
    val: u32,
  },
  /// A numeric operation between built-in numbers.
  Opx {
    op: Op,
    fst: Box<Term>,
    snd: Box<Term>,
  },
  Tup {
    fst: Box<Term>,
    snd: Box<Term>,
  },
}

#[derive(Debug, Clone)]
pub enum LetPat {
  Var(Name),
  Tup(Option<Name>, Option<Name>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
  ADD,
  SUB,
  MUL,
  DIV,
  MOD,
  EQ,
  NE,
  LT,
  GT,
  AND,
  OR,
  XOR,
  NOT,
  LSH,
  RSH,
}

/// A user defined  datatype
#[derive(Debug, Clone, Default)]
pub struct Adt {
  pub ctrs: BTreeMap<Name, usize>,
}

#[derive(Debug, PartialEq, Eq, Clone, Shrinkwrap, Hash, PartialOrd, Ord, From, Into, Display)]
pub struct Name(pub String);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Shrinkwrap, Hash, PartialOrd, Ord, From, Into, Default)]
pub struct DefId(pub Val);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Shrinkwrap, Hash, PartialOrd, Ord, From, Into)]
pub struct VarId(pub Val);

pub fn var_id_to_name(mut var_id: Val) -> Name {
  let mut name = String::new();
  loop {
    let c = (var_id % 26) as u8 + b'a';
    name.push(c as char);
    var_id /= 26;
    if var_id == 0 {
      break;
    }
  }
  Name(name)
}

impl Name {
  pub fn new(value: &str) -> Self {
    Name(value.to_string())
  }
}

// TODO: We use this workaround because hvm-core's val_to_name function doesn't work with value 0
impl DefId {
  pub fn to_internal(self) -> Val {
    *self + 1
  }

  pub fn from_internal(val: Val) -> Self {
    Self(val - 1)
  }
}

impl Book {
  pub fn new() -> Self {
    Default::default()
  }
}

impl DefNames {
  pub const ENTRY_POINT: &'static str = "main";
  pub const HVM1_ENTRY_POINT: &'static str = "Main";

  pub fn new() -> Self {
    Default::default()
  }

  pub fn name(&self, def_id: &DefId) -> Option<&Name> {
    self.map.get_by_left(def_id)
  }

  pub fn def_id(&self, name: &Name) -> Option<DefId> {
    self.map.get_by_right(name).copied()
  }

  pub fn contains_name(&self, name: &Name) -> bool {
    self.map.contains_right(name)
  }

  pub fn contains_def_id(&self, def_id: &DefId) -> bool {
    self.map.contains_left(def_id)
  }

  pub fn insert(&mut self, name: Name) -> DefId {
    let def_id = self.id_count;
    self.id_count.0 += 1;
    match self.map.insert(def_id, name) {
      Overwritten::Neither => def_id,
      _ => todo!("Overwritting name-id pairs not supported"),
    }
  }
}

impl Term {
  pub fn to_string(&self, def_names: &DefNames) -> String {
    match self {
      Term::Lam { nam, bod } => {
        format!("λ{} {}", nam.clone().unwrap_or(Name::new("*")), bod.to_string(def_names))
      }
      Term::Var { nam } => format!("{nam}"),
      Term::Chn { nam, bod } => format!("λ${} {}", nam, bod.to_string(def_names)),
      Term::Lnk { nam } => format!("${nam}"),
      Term::Let { pat, val, nxt } => {
        format!("let {} = {}; {}", pat, val.to_string(def_names), nxt.to_string(def_names))
      }
      Term::Ref { def_id } => format!("{}", def_names.name(def_id).unwrap()),
      Term::App { fun, arg } => format!("({} {})", fun.to_string(def_names), arg.to_string(def_names)),
      Term::Match { cond, zero, succ } => {
        // Only the Lambda case represents a valid match construction,
        // but we still have to display invalid ones
        let (pred, succ) = match succ.as_ref() {
          Term::Lam { nam, bod } => (nam, bod),
          _ => (&None, succ),
        };

        format!(
          "match {} {{ 0: {}; 1+{}: {} }}",
          cond.to_string(def_names),
          zero.to_string(def_names),
          pred.clone().unwrap_or(Name::new("*")),
          succ.to_string(def_names),
        )
      }
      Term::Dup { fst, snd, val, nxt } => format!(
        "dup {} {} = {}; {}",
        fst.as_ref().map(|x| x.as_str()).unwrap_or("*"),
        snd.as_ref().map(|x| x.as_str()).unwrap_or("*"),
        val.to_string(def_names),
        nxt.to_string(def_names)
      ),
      Term::Sup { fst, snd } => format!("{{{} {}}}", fst.to_string(def_names), snd.to_string(def_names)),
      Term::Era => "*".to_string(),
      Term::Num { val } => format!("{val}"),
      Term::Opx { op, fst, snd } => {
        format!("({} {} {})", op, fst.to_string(def_names), snd.to_string(def_names))
      }
      Term::Tup { fst, snd } => format!("({}, {})", fst.to_string(def_names), snd.to_string(def_names)),
    }
  }

  /// Make a call term by folding args around a called function term with applications.
  pub fn call(called: Term, args: impl IntoIterator<Item = Term>) -> Self {
    args.into_iter().fold(called, |acc, arg| Term::App { fun: Box::new(acc), arg: Box::new(arg) })
  }

  /// Substitute the occurences of a variable in a term with the given term.
  pub fn subst(&mut self, from: &Name, to: &Term) {
    match self {
      Term::Lam { nam: Some(nam), .. } if nam == from => (),
      Term::Lam { bod, .. } => bod.subst(from, to),
      Term::Var { nam } if nam == from => *self = to.clone(),
      Term::Var { .. } => (),
      // Only substitute scoped variables.
      Term::Chn { bod, .. } => bod.subst(from, to),
      Term::Lnk { .. } => (),
      Term::Let { pat: LetPat::Var(nam), val, nxt } => {
        val.subst(from, to);
        if nam != from {
          nxt.subst(from, to);
        }
      }
      Term::Let { pat: LetPat::Tup(fst, snd), val, nxt } => {
        val.subst(from, to);
        if fst.as_ref().map_or(true, |fst| fst != from) && snd.as_ref().map_or(true, |snd| snd != from) {
          nxt.subst(from, to);
        }
      }
      Term::Match { cond, zero, succ, .. } => {
        cond.subst(from, to);
        zero.subst(from, to);
        succ.subst(from, to);
      }
      Term::Ref { .. } => (),
      Term::App { fun, arg } => {
        fun.subst(from, to);
        arg.subst(from, to);
      }
      Term::Dup { fst, snd, val, nxt } => {
        val.subst(from, to);
        if fst.as_ref().map_or(true, |fst| fst != from) && snd.as_ref().map_or(true, |snd| snd != from) {
          nxt.subst(from, to);
        }
      }
      Term::Sup { fst, snd } => {
        fst.subst(from, to);
        snd.subst(from, to);
      }
      Term::Era => (),
      Term::Num { .. } => (),
      Term::Opx { fst, snd, .. } => {
        fst.subst(from, to);
        snd.subst(from, to);
      }
      Term::Tup { fst, snd } => {
        fst.subst(from, to);
        snd.subst(from, to);
      }
    }
  }
}

impl fmt::Display for LetPat {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      LetPat::Var(nam) => write!(f, "{}", nam),
      LetPat::Tup(fst, snd) => {
        write!(
          f,
          "({}, {})",
          fst.as_ref().map(|s| s.to_string()).unwrap_or("*".to_string()),
          snd.as_ref().map(|s| s.to_string()).unwrap_or("*".to_string()),
        )
      }
    }
  }
}

impl Rule {
  pub fn to_string(&self, def_names: &DefNames) -> String {
    let Rule { def_id, pats, body } = self;
    format!(
      "({}{}) = {}",
      def_names.name(def_id).unwrap(),
      pats.iter().map(|x| format!(" {x}")).join(""),
      body.to_string(def_names)
    )
  }

  pub fn arity(&self) -> usize {
    self.pats.len()
  }
}

impl Definition {
  pub fn to_string(&self, def_names: &DefNames) -> String {
    self.rules.iter().map(|x| x.to_string(def_names)).join("\n")
  }

  pub fn arity(&self) -> usize {
    self.rules[0].arity()
  }
}

impl fmt::Display for Book {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.defs.iter().map(|(_, x)| x.to_string(&self.def_names)).join("\n\n"))
  }
}

impl From<&RulePat> for Term {
  fn from(value: &RulePat) -> Self {
    match value {
      RulePat::Ctr(nam, args) => Term::call(Term::Var { nam: nam.clone() }, args.iter().map(Term::from)),
      RulePat::Var(nam) => Term::Var { nam: nam.clone() },
    }
  }
}

impl fmt::Display for RulePat {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      RulePat::Ctr(name, pats) => write!(f, "({}{})", name, pats.iter().map(|p| format!(" {p}")).join("")),
      RulePat::Var(nam) => write!(f, "{}", nam),
    }
  }
}

impl fmt::Display for Op {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Op::ADD => write!(f, "+"),
      Op::SUB => write!(f, "-"),
      Op::MUL => write!(f, "*"),
      Op::DIV => write!(f, "/"),
      Op::MOD => write!(f, "%"),
      Op::EQ => write!(f, "=="),
      Op::NE => write!(f, "!="),
      Op::LT => write!(f, "<"),
      Op::GT => write!(f, ">"),
      Op::AND => write!(f, "&"),
      Op::OR => write!(f, "|"),
      Op::XOR => write!(f, "^"),
      Op::NOT => write!(f, "~"),
      Op::LSH => write!(f, "<<"),
      Op::RSH => write!(f, ">>"),
    }
  }
}