use std::{
    collections::BTreeMap,
    fmt::{Debug, Display},
};

use graphql_parser::{
    query::{
        Definition, Document, Field, FragmentDefinition, FragmentSpread, InlineFragment,
        OperationDefinition, Query, Selection, SelectionSet, TypeCondition, VariableDefinition,
    },
    schema::{Directive, Text, Value},
};
use itertools::Itertools;

pub(crate) fn parse_document<'a, T>(doc: &'a Document<'a, T>) -> Vec<String>
where
    T: Text<'a> + Debug,
    T::Value: Display + Debug,
{
    let mut field_queries = Vec::new();
    let fragments: BTreeMap<String, &FragmentDefinition<'_, T>> = doc
        .definitions
        .iter()
        .filter_map(|def| match def {
            Definition::Fragment(f) => Some((f.name.to_string(), f)),
            _ => None,
        })
        .collect();

    for query in doc.definitions.iter().filter_map(|def| match def {
        Definition::Operation(OperationDefinition::Query(query)) => Some(query),
        _ => None,
    }) {
        handle_query(query, &mut field_queries, &fragments);
    }

    field_queries
}

fn handle_query<'a, 'b, T>(
    query: &Query<'a, T>,
    field_queries: &mut Vec<String>,
    fragments: &'b BTreeMap<String, &FragmentDefinition<'a, T>>,
) -> anyhow::Result<()>
where
    T: Text<'a> + Debug,
    T::Value: Display + Debug,
{
    handle_selection_set(
        &Vec::from([format!(
            "query {}({}) {}",
            query
                .name
                .as_ref()
                .map(|s| s.to_string())
                .unwrap_or_default(),
            variable_definitions_to_str(&query.variable_definitions),
            directives_to_str(&query.directives),
        )]),
        &query.selection_set,
        field_queries,
        fragments,
    )
}

fn handle_selection_set<'a, 'b, T>(
    path: &[String],
    ss: &SelectionSet<'a, T>,
    field_queries: &mut Vec<String>,
    fragments: &'b BTreeMap<String, &FragmentDefinition<'a, T>>,
) -> anyhow::Result<()>
where
    T: Text<'a> + Debug,
    T::Value: Display + Debug,
{
    for item in ss.items.iter() {
        match item {
            Selection::Field(field) => handle_field(path, field, field_queries, fragments)?,
            Selection::FragmentSpread(spread) => {
                handle_fragment_spread(path, spread, field_queries, fragments)?
            }
            Selection::InlineFragment(fragment) => {
                handle_inline_fragment(path, fragment, field_queries, fragments)?
            }
        }
    }

    Ok(())
}

fn handle_field<'a, 'b, T>(
    path: &[String],
    field: &Field<'a, T>,
    field_queries: &mut Vec<String>,
    fragments: &'b BTreeMap<String, &FragmentDefinition<'a, T>>,
) -> anyhow::Result<()>
where
    T: Text<'a> + Debug,
    T::Value: Display + Debug,
{
    let mut path = Vec::from(path);
    path.push(format!(
        "{}{}{} {}",
        field
            .alias
            .as_ref()
            .map(|alias| format!("{}: ", alias))
            .unwrap_or_default(),
        field.name,
        arguments_to_str(&field.arguments),
        directives_to_str(&field.directives),
    ));

    if field.selection_set.items.is_empty() {
        // Leaf node; handle accordingly.
        field_queries.push(path_to_query(&path)?);
    } else {
        handle_selection_set(&path, &field.selection_set, field_queries, fragments)?;
    }

    Ok(())
}

fn handle_fragment_spread<'a, 'b, T>(
    path: &[String],
    spread: &FragmentSpread<'a, T>,
    field_queries: &mut Vec<String>,
    fragments: &'b BTreeMap<String, &FragmentDefinition<'a, T>>,
) -> anyhow::Result<()>
where
    T: Text<'a> + Debug,
    T::Value: Display + Debug,
{
    let fragment = match fragments.get(&spread.fragment_name.to_string()) {
        Some(fragment) => fragment,
        None => anyhow::bail!(
            "cannot find fragment with name {}",
            spread.fragment_name.to_string()
        ),
    };

    let mut path = Vec::from(path);
    path.push(format!(
        "... {} {}",
        fragment.type_condition,
        directives_to_str(&fragment.directives)
    ));

    handle_selection_set(&path, &fragment.selection_set, field_queries, fragments)
}

fn handle_inline_fragment<'a, 'b, T>(
    path: &[String],
    fragment: &InlineFragment<'a, T>,
    field_queries: &mut Vec<String>,
    fragments: &'b BTreeMap<String, &FragmentDefinition<'a, T>>,
) -> anyhow::Result<()>
where
    T: Text<'a> + Debug,
    T::Value: Display + Debug,
{
    let mut path = Vec::from(path);
    path.push(match &fragment.type_condition {
        Some(TypeCondition::On(cond)) => format!(
            "... on {} {}",
            cond,
            directives_to_str(&fragment.directives)
        ),
        None => "".to_string(),
    });

    handle_selection_set(&path, &fragment.selection_set, field_queries, fragments)
}

fn path_to_query(path: &[String]) -> anyhow::Result<String> {
    Ok(format!(
        "{}",
        graphql_parser::parse_query::<String>(&format!(
            "{}{}",
            path.iter().join(" { "),
            path.iter().skip(1).map(|_| "}").join(" "),
        ))?,
    ))
}

fn arguments_to_str<'a, T>(args: &[(T::Value, Value<'a, T>)]) -> String
where
    T: Text<'a> + Debug,
    T::Value: Display + Debug,
{
    if args.is_empty() {
        "".to_string()
    } else {
        format!(
            "({})",
            args.iter()
                .map(|(name, value)| format!("{}: {}", name, value))
                .join(", ")
        )
    }
}

fn directives_to_str<'a, T>(dirs: &[Directive<'a, T>]) -> String
where
    T: Text<'a> + Debug,
    T::Value: Display + Debug,
{
    dirs.iter().map(|dir| format!("{}", &dir)).join(" ")
}

fn variable_definitions_to_str<'a, T>(defs: &[VariableDefinition<'a, T>]) -> String
where
    T: Text<'a> + Debug,
    T::Value: Display + Debug,
{
    defs.iter().map(|var| format!("{}", &var)).join(", ")
}
