use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    character::complete::{char, digit1, multispace0},
    combinator::map,
    multi::separated_list0,
    sequence::{delimited, preceded, tuple},
    IResult,
};

/*
Format:
    PlacementSpec         => "" | KeyVal;PlacementSpec
    KeyVal                => SourceTag=ConstraintExpr
    SourceTag             => String
    ConstraintExpr        => NumContainers | NumContainers, Constraint
    Constraint            => SingleConstraint | CompositeConstraint
    SingleConstraint      => "IN",Scope,TargetTag | "NOTIN",Scope,TargetTag | "CARDINALITY",Scope,TargetTag,MinCard,MaxCard
    CompositeConstraint   => AND(ConstraintList) | OR(ConstraintList)
    ConstraintList        => Constraint | Constraint:ConstraintList
    NumContainers         => int
    Scope                 => "NODE" | "RACK"
    TargetTag             => String
    MinCard               => int
    MaxCard               => int
*/

#[derive(Debug, Clone)]
pub(crate) struct PlacementSpecList {
    pub specs: Vec<PlacementSpec>,
}

#[derive(Debug, Clone)]
pub(crate) struct PlacementSpec {
    pub source_tag: String,
    pub constraint_expr: ConstraintExpr,
}

#[derive(Debug, Clone)]
pub(crate) enum ConstraintExpr {
    NumContainers(i32),
    NumContainersWithConstraint(i32, Constraint),
}

#[derive(Debug, Clone)]
pub(crate) enum Constraint {
    Single(SingleConstraint),
    Composite(CompositeConstraint),
}

#[derive(Debug, Clone)]
pub(crate) enum SingleConstraint {
    In {
        scope: Scope,
        target_tag: String,
    },
    NotIn {
        scope: Scope,
        target_tag: String,
    },
    Cardinality {
        scope: Scope,
        target_tag: String,
        min_card: i32,
        max_card: i32,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum CompositeConstraint {
    And(Vec<Constraint>),
    Or(Vec<Constraint>),
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Scope {
    Node,
    Rack,
}

impl AsRef<str> for Scope {
    fn as_ref(&self) -> &str {
        match self {
            Scope::Node => "NODE",
            Scope::Rack => "RACK",
        }
    }
}

pub(crate) fn parse_placement_spec_list(input: &str) -> IResult<&str, PlacementSpecList> {
    map(
        separated_list0(char(':'), parse_placement_spec),
        |placement_specs| PlacementSpecList {
            specs: placement_specs,
        },
    )(input)
}

fn parse_placement_spec(input: &str) -> IResult<&str, PlacementSpec> {
    let (rest, (source_tag, _, constraint_expr)) = tuple((
        parse_source_tag,
        preceded(multispace0, char('=')),
        parse_constraint_expr,
    ))(input)?;

    Ok((
        rest,
        PlacementSpec {
            source_tag,
            constraint_expr,
        },
    ))
}

fn parse_source_tag(input: &str) -> IResult<&str, String> {
    take_until("=")(input).map(|(rest, source_tag)| (rest, source_tag.to_string()))
}

fn parse_num_containers(input: &str) -> IResult<&str, ConstraintExpr> {
    map(digit1, |num: &str| {
        ConstraintExpr::NumContainers(num.parse().unwrap_or(0))
    })(input)
}

fn parse_num_containers_with_constraint(input: &str) -> IResult<&str, ConstraintExpr> {
    let (rest, (num_containers, _, constraint)) = tuple((
        digit1,
        preceded(multispace0, char(',')),
        preceded(multispace0, parse_constraint),
    ))(input)?;

    Ok((
        rest,
        ConstraintExpr::NumContainersWithConstraint(
            num_containers.parse().unwrap_or(0),
            constraint,
        ),
    ))
}

fn parse_constraint_expr(input: &str) -> IResult<&str, ConstraintExpr> {
    alt((parse_num_containers_with_constraint, parse_num_containers))(input)
}

fn parse_single_constraint(input: &str) -> IResult<&str, Constraint> {
    alt((
        parse_in_constraint,
        parse_notin_constraint,
        parse_cardinality_constraint,
    ))(input)
}

fn parse_in_constraint(input: &str) -> IResult<&str, Constraint> {
    let (rest, (_, _, scope, _, target_tag)) = tuple((
        tag("IN"),
        preceded(multispace0, char(',')),
        preceded(multispace0, parse_scope),
        preceded(multispace0, char(',')),
        preceded(multispace0, parse_target_tag),
    ))(input)?;

    Ok((
        rest,
        Constraint::Single(SingleConstraint::In { scope, target_tag }),
    ))
}

fn parse_notin_constraint(input: &str) -> IResult<&str, Constraint> {
    let (rest, (_, _, scope, _, target_tag)) = tuple((
        tag("NOTIN"),
        preceded(multispace0, char(',')),
        preceded(multispace0, parse_scope),
        preceded(multispace0, char(',')),
        preceded(multispace0, parse_target_tag),
    ))(input)?;

    Ok((
        rest,
        Constraint::Single(SingleConstraint::NotIn { scope, target_tag }),
    ))
}

fn parse_min_card(input: &str) -> IResult<&str, i32> {
    map(digit1, |num: &str| num.parse().unwrap_or(0))(input)
}

fn parse_max_card(input: &str) -> IResult<&str, i32> {
    map(digit1, |num: &str| num.parse().unwrap_or(0))(input)
}

fn parse_cardinality_constraint(input: &str) -> IResult<&str, Constraint> {
    let (rest, (_, _, scope, _, target_tag, _, min_card, _, max_card)) = tuple((
        tag("CARDINALITY"),
        preceded(multispace0, char(',')),
        preceded(multispace0, parse_scope),
        preceded(multispace0, char(',')),
        preceded(multispace0, parse_target_tag),
        preceded(multispace0, char(',')),
        preceded(multispace0, parse_min_card),
        preceded(multispace0, char(',')),
        preceded(multispace0, parse_max_card),
    ))(input)?;

    Ok((
        rest,
        Constraint::Single(SingleConstraint::Cardinality {
            scope,
            target_tag,
            min_card,
            max_card,
        }),
    ))
}

fn parse_composite_constraint(input: &str) -> IResult<&str, Constraint> {
    alt((parse_and_constraint, parse_or_constraint))(input)
}

fn parse_and_constraint(input: &str) -> IResult<&str, Constraint> {
    let (rest, (_, constraints)) = tuple((
        tag("AND"),
        delimited(
            char('('),
            separated_list0(char(':'), parse_constraint),
            char(')'),
        ),
    ))(input)?;

    Ok((
        rest,
        Constraint::Composite(CompositeConstraint::And(constraints)),
    ))
}

fn parse_or_constraint(input: &str) -> IResult<&str, Constraint> {
    let (rest, (_, constraints)) = tuple((
        tag("OR"),
        delimited(
            char('('),
            separated_list0(char(':'), parse_constraint),
            char(')'),
        ),
    ))(input)?;

    Ok((
        rest,
        Constraint::Composite(CompositeConstraint::Or(constraints)),
    ))
}

fn parse_constraint(input: &str) -> IResult<&str, Constraint> {
    alt((parse_composite_constraint, parse_single_constraint))(input)
}

fn parse_scope(input: &str) -> IResult<&str, Scope> {
    alt((
        map(tag("NODE"), |_| Scope::Node),
        map(tag("RACK"), |_| Scope::Rack),
    ))(input)
}

fn parse_target_tag(input: &str) -> IResult<&str, String> {
    take_while1(|c: char| c.is_alphanumeric())(input)
        .map(|(rest, target_tag)| (rest, target_tag.to_string()))
}
