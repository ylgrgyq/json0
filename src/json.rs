use std::{fmt::Display, mem};

use crate::{
    common::Validation,
    error::{JsonError, Result},
    operation::{Appliable, OperationComponent, Operator},
    path::{Path, PathElement},
};

use serde_json::Value;

trait Routable {
    fn route_get(&self, paths: &Path) -> Result<Option<&Value>>;

    fn route_get_mut(&mut self, paths: &Path) -> Result<Option<&mut Value>>;
}

impl Routable for Value {
    fn route_get(&self, paths: &Path) -> Result<Option<&Value>> {
        match self {
            Value::Array(array) => array.route_get(paths),
            Value::Object(obj) => obj.route_get(paths),
            Value::Null => Ok(None),
            _ => {
                if paths.is_empty() {
                    Ok(Some(self))
                } else {
                    Err(JsonError::BadPath)
                }
            }
        }
    }

    fn route_get_mut(&mut self, paths: &Path) -> Result<Option<&mut Value>> {
        match self {
            Value::Array(array) => array.route_get_mut(paths),
            Value::Object(obj) => obj.route_get_mut(paths),
            _ => {
                if paths.is_empty() {
                    Ok(Some(self))
                } else {
                    Err(JsonError::BadPath)
                }
            }
        }
    }
}

impl Routable for serde_json::Map<String, serde_json::Value> {
    fn route_get(&self, paths: &Path) -> Result<Option<&Value>> {
        let k = paths.first_key_path().ok_or(JsonError::BadPath)?;
        if let Some(v) = self.get(k) {
            let next_level = paths.next_level();
            if next_level.is_empty() {
                Ok(Some(v))
            } else {
                v.route_get(&next_level)
            }
        } else {
            Ok(None)
        }
    }

    fn route_get_mut(&mut self, paths: &Path) -> Result<Option<&mut Value>> {
        let k = paths.first_key_path().ok_or(JsonError::BadPath)?;
        if let Some(v) = self.get_mut(k) {
            let next_level = paths.next_level();
            if next_level.is_empty() {
                Ok(Some(v))
            } else {
                v.route_get_mut(&next_level)
            }
        } else {
            Ok(None)
        }
    }
}

impl Routable for Vec<serde_json::Value> {
    fn route_get(&self, paths: &Path) -> Result<Option<&Value>> {
        let i = paths.first_index_path().ok_or(JsonError::BadPath)?;
        if let Some(v) = self.get(*i) {
            let next_level = paths.next_level();
            if next_level.is_empty() {
                Ok(Some(v))
            } else {
                v.route_get(&next_level)
            }
        } else {
            Ok(None)
        }
    }

    fn route_get_mut(&mut self, paths: &Path) -> Result<Option<&mut Value>> {
        let i = paths.first_index_path().ok_or(JsonError::BadPath)?;
        if let Some(v) = self.get_mut(*i) {
            let next_level = paths.next_level();
            if next_level.is_empty() {
                Ok(Some(v))
            } else {
                v.route_get_mut(&next_level)
            }
        } else {
            Ok(None)
        }
    }
}

impl Appliable for Value {
    fn apply(&mut self, paths: Path, operator: Operator) -> Result<()> {
        if paths.len() > 1 {
            let (left, right) = paths.split_at(paths.len() - 1);
            return self
                .route_get_mut(&left)?
                .ok_or(JsonError::BadPath)?
                .apply(right, operator);
        }
        match self {
            Value::Array(array) => array.apply(paths, operator),
            Value::Object(obj) => obj.apply(paths, operator),
            Value::Number(n) => match operator {
                Operator::AddNumber(v) => {
                    let new_v = n.as_u64().unwrap() + v.as_u64().unwrap();
                    let serde_v = serde_json::to_value(new_v)?;
                    _ = mem::replace(self, serde_v);
                    Ok(())
                }
                _ => {
                    return Err(JsonError::InvalidOperation(
                        "Only AddNumber operation can apply to a Number JSON Value".into(),
                    ));
                }
            },
            _ => {
                return Err(JsonError::InvalidOperation(
                    "Operation can only apply on array or object".into(),
                ));
            }
        }
    }
}

impl Appliable for serde_json::Map<String, serde_json::Value> {
    fn apply(&mut self, paths: Path, operator: Operator) -> Result<()> {
        assert!(paths.len() == 1);

        let k = paths.first_key_path().ok_or(JsonError::BadPath)?;
        let target_value = self.get_mut(k);
        match &operator {
            Operator::AddNumber(v) => {
                if let Some(old_v) = target_value {
                    old_v.apply(paths, operator)
                } else {
                    self.insert(k.clone(), v.clone());
                    Ok(())
                }
            }
            Operator::ObjectInsert(v) => {
                self.insert(k.clone(), v.clone());
                Ok(())
            }
            Operator::ObjectDelete(delete_v) => {
                if let Some(target_v) = target_value {
                    if target_v.eq(&delete_v) {
                        self.remove(k);
                    }
                }
                Ok(())
            }
            Operator::ObjectReplace(new_v, old_v) => {
                if let Some(target_v) = target_value {
                    if target_v.eq(&old_v) {
                        self.insert(k.clone(), new_v.clone());
                    }
                }
                Ok(())
            }
            _ => Err(JsonError::BadPath),
        }
    }
}

impl Appliable for Vec<serde_json::Value> {
    fn apply(&mut self, paths: Path, operator: Operator) -> Result<()> {
        assert!(paths.len() == 1);

        let index = paths.first_index_path().ok_or(JsonError::BadPath)?;
        let target_value = self.get_mut(*index);
        match &operator {
            Operator::AddNumber(v) => {
                if let Some(old_v) = target_value {
                    match old_v {
                        Value::Number(n) => {
                            let new_v = n.as_u64().unwrap() + v.as_u64().unwrap();
                            let serde_v = serde_json::to_value(new_v)?;
                            self[*index] = serde_v;
                            Ok(())
                        }
                        _ => return Err(JsonError::BadPath),
                    }
                } else {
                    self[*index] = v.clone();
                    Ok(())
                }
            }
            Operator::ListInsert(v) => {
                if *index > self.len() {
                    self.push(v.clone())
                } else {
                    self.insert(*index, v.clone());
                }
                Ok(())
            }
            Operator::ListDelete(delete_v) => {
                if let Some(target_v) = target_value {
                    if target_v.eq(&delete_v) {
                        self.remove(*index);
                    }
                }
                Ok(())
            }
            Operator::ListReplace(new_v, old_v) => {
                if let Some(target_v) = target_value {
                    if target_v.eq(&old_v) {
                        self[*index] = new_v.clone();
                    }
                }
                Ok(())
            }
            Operator::ListMove(new_index) => {
                if let Some(target_v) = target_value {
                    if *index != *new_index {
                        let new_v = target_v.clone();
                        self.remove(*index);
                        self.insert(*new_index, new_v);
                    }
                }
                Ok(())
            }
            _ => Err(JsonError::BadPath),
        }
    }
}

pub type Operation = Vec<OperationComponent>;

impl Validation for Vec<OperationComponent> {
    fn validates(&self) -> Result<()> {
        for op in self.iter() {
            op.validates()?;
        }
        Ok(())
    }
}

#[derive(PartialEq)]
pub enum TransformSide {
    LEFT,
    RIGHT,
}
pub struct Transformer {}

impl Transformer {
    fn transform_component(
        &self,
        base_op: &OperationComponent,
        new_op: &OperationComponent,
        side: TransformSide,
    ) -> Result<OperationComponent> {
        let mut new_op = new_op.clone();

        let base_op_new_op_common = self.transform_operating_path(base_op, &new_op);
        let new_op_base_op_common = self.transform_operating_path(&new_op, base_op);
        let mut new_op_len = new_op.path.len();
        let mut base_op_len = base_op.path.len();

        if let Operator::AddNumber(_) = new_op.operator {
            new_op_len += 1;
        }

        if let Operator::AddNumber(_) = base_op.operator {
            base_op_len += 1;
        }

        if new_op_base_op_common.is_some()
            && base_op_len > new_op_len
            && base_op
                .path
                .get(new_op_base_op_common.unwrap())
                .unwrap()
                .eq(new_op.path.get(new_op_base_op_common.unwrap()).unwrap())
        {
            match &mut new_op.operator {
                Operator::ListDelete(v)
                | Operator::ListReplace(_, v)
                | Operator::ObjectDelete(v)
                | Operator::ObjectReplace(_, v) => {
                    let (_, p2) = base_op.path.split_at(new_op_len);
                    v.apply(p2, base_op.operator.clone())?;
                }
                _ => {}
            }
        }

        if let Some(common) = base_op_new_op_common {
            let path_length_equal = base_op.path.len() == new_op.path.len();

            match base_op.operator {
                Operator::ListInsert(_) => match new_op.operator {
                    Operator::ListInsert(_) => {
                        if path_length_equal
                            && new_op
                                .path
                                .get(common)
                                .unwrap()
                                .eq(base_op.path.get(common).unwrap())
                        {
                            if side == TransformSide::RIGHT {
                                let path_elems = new_op.path.get_mut_elements();
                                if let PathElement::Index(i) =
                                    path_elems.pop().ok_or(JsonError::BadPath)?
                                {
                                    path_elems.push(PathElement::Index(i + 1))
                                } else {
                                    return Err(JsonError::BadPath);
                                }
                            }
                        } else if base_op.path.get(common).unwrap()
                            <= new_op.path.get(common).unwrap()
                        {
                            let path_elems = new_op.path.get_mut_elements();
                            if let PathElement::Index(i) =
                                path_elems.pop().ok_or(JsonError::BadPath)?
                            {
                                path_elems.push(PathElement::Index(i + 1))
                            } else {
                                return Err(JsonError::BadPath);
                            }
                        }
                        return Ok(new_op);
                    }
                    _ => return Ok(new_op),
                },
                Operator::ListDelete(_) => todo!(),
                Operator::ListReplace(_, _) => todo!(),
                Operator::ListMove(_) => todo!(),
                Operator::ObjectInsert(_) => todo!(),
                Operator::ObjectDelete(_) => todo!(),
                Operator::ObjectReplace(_, _) => todo!(),
                _ => return Ok(new_op),
            }
        }

        todo!()
    }

    fn transform_component2(
        &self,
        base_op: &OperationComponent,
        new_op: &OperationComponent,
        side: TransformSide,
    ) -> Result<OperationComponent> {
        let mut new_op = new_op.clone();

        let max_common_path = base_op.path.max_common_path(&new_op.path);
        if max_common_path.is_empty() {
            // new_op and base_op does not have common path
            return Ok(new_op);
        }

        // [1,2,3], [1,2,5] max_common_path + 1 = op path len
        // [1,2,3], [1,2,5,8]
        // [1,2,3], [1,2,3,5] max_common_path == op path len
        // [1,2,3,7,8], [1,2,1]
        // [1,2,3,7,8], [1,2,3]
        let new_operate_path = new_op.operate_path();
        let base_operate_path = base_op.operate_path();
        if max_common_path.len() < new_operate_path.len()
            && max_common_path.len() < base_operate_path.len()
        {
            // common path must be equal to new_op's or base_op's operate path
            // or base_op and new_op is operating on orthogonal value
            // they don't need transform
            return Ok(new_op);
        }

        if base_operate_path.len() > new_operate_path.len() {
            // if base_op's path is longger and contains new_op's path, new_op should include base_op's effect
            if max_common_path.len() == new_op.path.len() {
                new_op.consume(&max_common_path, &base_op)?;
            }
            // new_op, base_op
            // {a: [1,[1,2,3]]}
            // [a,1,1], li, [a,1,3], ld
            // [1,2,3,7,8], [1,2,3]
            return Ok(new_op);
        }

        // from here, new_op's path is shorter or equal to base_op
        let same_operand = new_operate_path.len() == base_operate_path.len();
        match base_op.operator {
            Operator::ListInsert(_) => match new_op.operator {
                Operator::ListInsert(_) => {
                    if same_operand && max_common_path.len() == new_op.path.len() {
                        if side == TransformSide::RIGHT {
                            let path_elems = new_op.path.get_mut_elements();
                            if let PathElement::Index(i) =
                                path_elems.pop().ok_or(JsonError::BadPath)?
                            {
                                path_elems.push(PathElement::Index(i + 1))
                            } else {
                                return Err(JsonError::BadPath);
                            }
                        }
                    } else if base_op.path.last().unwrap() <= new_op.path.last().unwrap() {
                        let path_elems = new_op.path.get_mut_elements();
                        if let PathElement::Index(i) = path_elems.pop().ok_or(JsonError::BadPath)? {
                            path_elems.push(PathElement::Index(i + 1))
                        } else {
                            return Err(JsonError::BadPath);
                        }
                    }
                }
                Operator::ListDelete(_) => {
                    if base_op.path.last().unwrap() <= new_op.path.last().unwrap() {
                        let path_elems = new_op.path.get_mut_elements();
                        if let PathElement::Index(i) = path_elems.pop().ok_or(JsonError::BadPath)? {
                            path_elems.push(PathElement::Index(i + 1))
                        } else {
                            return Err(JsonError::BadPath);
                        }
                    }
                }
                Operator::ListReplace(_, _) => {
                    if base_op.path.last().unwrap() <= new_op.path.last().unwrap() {
                        let path_elems = new_op.path.get_mut_elements();
                        if let PathElement::Index(i) = path_elems.pop().ok_or(JsonError::BadPath)? {
                            path_elems.push(PathElement::Index(i + 1))
                        } else {
                            return Err(JsonError::BadPath);
                        }
                    }
                }
                Operator::ListMove(_) => todo!(),
                _ => return Ok(new_op),
            },
            Operator::ListDelete(_) => todo!(),
            Operator::ListReplace(_, _) => todo!(),
            Operator::ListMove(_) => todo!(),
            Operator::ObjectInsert(_) => todo!(),
            Operator::ObjectDelete(_) => todo!(),
            Operator::ObjectReplace(_, _) => todo!(),
            _ => return Ok(new_op),
        }

        Ok(new_op)
    }

    pub fn append(&self, operation: &mut Operation, op: &OperationComponent) -> Result<()> {
        op.validates()?;

        if let Operator::ListMove(m) = op.operator {
            if op
                .path
                .get(op.path.len() - 1)
                .unwrap()
                .eq(&PathElement::Index(m))
            {
                return Ok(());
            }
        }

        if operation.is_empty() {
            operation.push(op.clone());
            return Ok(());
        }

        let last = operation.last_mut().unwrap();
        if last.path.eq(&op.path) && last.merge(op) {
            if last.operator.eq(&Operator::Noop()) {
                operation.pop();
            }
            return Ok(());
        }
        operation.push(op.clone());
        Ok(())
    }

    pub fn invert(&self, operation: &OperationComponent) -> Result<OperationComponent> {
        operation.validates()?;

        let mut path = operation.path.clone();
        let operator = match &operation.operator {
            Operator::Noop() => Operator::Noop(),
            Operator::AddNumber(n) => {
                Operator::AddNumber(serde_json::to_value(-n.as_i64().unwrap()).unwrap())
            }
            Operator::ListInsert(v) => Operator::ListDelete(v.clone()),
            Operator::ListDelete(v) => Operator::ListInsert(v.clone()),
            Operator::ListReplace(new_v, old_v) => {
                Operator::ListReplace(old_v.clone(), new_v.clone())
            }
            Operator::ListMove(new) => {
                let old_p = path.replace(path.len() - 1, PathElement::Index(new.clone()));
                if let Some(PathElement::Index(i)) = old_p {
                    Operator::ListMove(i)
                } else {
                    return Err(JsonError::BadPath);
                }
            }
            Operator::ObjectInsert(v) => Operator::ObjectDelete(v.clone()),
            Operator::ObjectDelete(v) => Operator::ObjectInsert(v.clone()),
            Operator::ObjectReplace(new_v, old_v) => {
                Operator::ObjectReplace(old_v.clone(), new_v.clone())
            }
        };
        Ok(OperationComponent::new(path, operator))
    }

    pub fn compose(&self, a: &Operation, b: &Operation) -> Result<Operation> {
        a.validates()?;

        let mut ret: Operation = a.clone();
        for op in b.iter() {
            self.append(&mut ret, &op)?;
        }

        Ok(ret)
    }

    fn transform_operating_path(
        &self,
        a: &OperationComponent,
        b: &OperationComponent,
    ) -> Option<usize> {
        let mut alen = a.path.len();
        let mut blen = b.path.len();
        if let Operator::AddNumber(_) = a.operator {
            alen += 1;
        }

        if let Operator::AddNumber(_) = b.operator {
            blen += 1;
        }

        if alen == 0 {
            return Some(0);
        }

        if blen == 0 {
            return None;
        }

        for (i, p) in a.path.get_elements().iter().enumerate() {
            if let Some(pb) = b.path.get(i) {
                if !p.eq(pb) {
                    return None;
                }
            } else {
                return None;
            }
        }
        Some(alen)
    }
}

#[derive(Clone)]
pub struct JSON {
    value: Value,
}

impl Display for JSON {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

impl JSON {
    pub fn from_str(input: &str) -> Result<JSON> {
        let value = serde_json::from_str(input)?;
        Ok(JSON { value })
    }

    pub fn apply(&mut self, operations: Vec<Operation>) -> Result<()> {
        for operation in operations {
            for op_comp in operation {
                self.value
                    .apply(op_comp.path.clone(), op_comp.operator.clone())?;
            }
        }
        Ok(())
    }

    pub fn get(&self, paths: &Path) -> Result<Option<&Value>> {
        self.value.route_get(paths)
    }
}

#[cfg(test)]
mod tests {

    use std::{
        io::{Read, Write},
        str::FromStr,
        vec,
    };

    use crate::path::Path;

    use super::*;
    use log::info;
    use test_log::test;

    #[test]
    fn test_route_get_by_path_only_has_object() {
        let json = JSON::from_str(r#"{"level1":"world", "level12":{"level2":"world2"}}"#).unwrap();

        // simple path with only object
        let paths = Path::from_str(r#"["level1"]"#).unwrap();
        assert_eq!(json.get(&paths).unwrap().unwrap().to_string(), r#""world""#);
        let paths = Path::from_str(r#"["level12", "level2"]"#).unwrap();
        assert_eq!(
            json.get(&paths).unwrap().unwrap().to_string(),
            r#""world2""#
        );
        let paths = Path::from_str(r#"["level3"]"#).unwrap();
        assert!(json.get(&paths).unwrap().is_none());

        // complex path with array
        let json =
            JSON::from_str(r#"{"level1":[1,{"hello":[1,[7,8]]}], "level12":"world"}"#).unwrap();
        let paths = Path::from_str(r#"["level1", 1, "hello"]"#).unwrap();

        assert_eq!(
            json.get(&paths).unwrap().unwrap().to_string(),
            r#"[1,[7,8]]"#
        );
    }

    #[test]
    fn test_route_get_by_path_has_array() {
        let json = JSON::from_str(r#"{"level1":["a","b"], "level12":[123, {"level2":["c","d"]}]}"#)
            .unwrap();
        // simple path
        let paths = Path::from_str(r#"["level1", 1]"#).unwrap();
        assert_eq!(json.get(&paths).unwrap().unwrap().to_string(), r#""b""#);
        let paths = Path::from_str(r#"["level12", 0]"#).unwrap();

        // complex path
        assert_eq!(json.get(&paths).unwrap().unwrap().to_string(), r#"123"#);
        let paths = Path::from_str(r#"["level12", 1, "level2"]"#).unwrap();
        assert_eq!(
            json.get(&paths).unwrap().unwrap().to_string(),
            r#"["c","d"]"#
        );
        let json =
            JSON::from_str(r#"{"level1":[1,{"hello":[1,[7,8]]}], "level12":"world"}"#).unwrap();
        let paths = Path::from_str(r#"["level1", 1, "hello", 1]"#).unwrap();

        assert_eq!(json.get(&paths).unwrap().unwrap().to_string(), r#"[7,8]"#);
    }

    #[test]
    fn test_apply_add_number() {
        let mut json = JSON::from_str("{\"level1\": 10}").unwrap();
        let operation_comp =
            OperationComponent::from_str("{\"p\":[\"level1\"], \"na\":100}").unwrap();
        json.apply(vec![vec![operation_comp.clone()]]).unwrap();

        assert_eq!(json.to_string(), r#"{"level1":110}"#);
    }

    #[test]
    fn test_object_insert() {
        let mut json = JSON::from_str(r#"{}"#).unwrap();
        // insert to empty object
        let operation_comp =
            OperationComponent::from_str(r#"{"p":["level1"], "oi":{"level2":{}}}"#).unwrap();
        json.apply(vec![vec![operation_comp]]).unwrap();
        assert_eq!(json.to_string(), r#"{"level1":{"level2":{}}}"#);

        // insert to inner object
        let operation_comp = OperationComponent::from_str(
            r#"{"p":["level1", "level2"], "oi":{"level3":[1, {"level4":{}}]}}"#,
        )
        .unwrap();
        json.apply(vec![vec![operation_comp]]).unwrap();
        assert_eq!(
            json.to_string(),
            r#"{"level1":{"level2":{"level3":[1,{"level4":{}}]}}}"#
        );

        // insert to deep inner object with number index in path
        let operation_comp = OperationComponent::from_str(
            r#"{"p":["level1", "level2", "level3", 1, "level4"], "oi":{"level5":[1, 2]}}"#,
        )
        .unwrap();
        json.apply(vec![vec![operation_comp]]).unwrap();
        assert_eq!(
            json.to_string(),
            r#"{"level1":{"level2":{"level3":[1,{"level4":{"level5":[1,2]}}]}}}"#
        );

        // replace key without compare
        let operation_comp = OperationComponent::from_str(
            r#"{"p":["level1", "level2", "level3", 1, "level4"], "oi":[3,4]}"#,
        )
        .unwrap();
        json.apply(vec![vec![operation_comp]]).unwrap();
        assert_eq!(
            json.to_string(),
            r#"{"level1":{"level2":{"level3":[1,{"level4":[3,4]}]}}}"#
        );
    }

    #[test]
    fn test_object_delete() {
        let origin_json = JSON::from_str(
            r#"{"level1":{"level2":{"level3":[1,{"level41":[1,2], "level42":[3,4]}]}}}"#,
        )
        .unwrap();

        // delete to deep inner object with number index in path
        let mut json = origin_json.clone();
        let operation_comp = OperationComponent::from_str(
            r#"{"p":["level1", "level2", "level3", 1, "level41"], "od":[1, 2]}"#,
        )
        .unwrap();
        json.apply(vec![vec![operation_comp]]).unwrap();
        assert_eq!(
            json.to_string(),
            r#"{"level1":{"level2":{"level3":[1,{"level42":[3,4]}]}}}"#
        );

        // delete to inner object
        let mut json = origin_json.clone();
        let operation_comp = OperationComponent::from_str(
            r#"{"p":["level1", "level2", "level3"], "od":[1,{"level41":[1,2], "level42":[3,4]}]}"#,
        )
        .unwrap();
        json.apply(vec![vec![operation_comp]]).unwrap();
        assert_eq!(json.to_string(), r#"{"level1":{"level2":{}}}"#);
    }

    #[test]
    fn test_object_replace() {
        let origin_json = JSON::from_str(
            r#"{"level1":{"level2":{"level3":[1,{"level41":[1,2], "level42":[3,4]}]}}}"#,
        )
        .unwrap();

        // replace deep inner object with number index in path
        let mut json = origin_json.clone();
        let operation_comp = OperationComponent::from_str(
            r#"{"p":["level1", "level2", "level3", 1, "level41"], "oi":{"5":"6"}, "od":[1, 2]}"#,
        )
        .unwrap();
        json.apply(vec![vec![operation_comp]]).unwrap();
        assert_eq!(
            json.to_string(),
            r#"{"level1":{"level2":{"level3":[1,{"level41":{"5":"6"},"level42":[3,4]}]}}}"#
        );

        // replace to inner object
        let mut json = origin_json.clone();
        let operation_comp = OperationComponent::from_str(
            r#"{"p":["level1", "level2"], "oi":"hello", "od":{"level3":[1,{"level41":[1,2], "level42":[3,4]}]}}"#,
        )
        .unwrap();
        json.apply(vec![vec![operation_comp]]).unwrap();
        assert_eq!(json.to_string(), r#"{"level1":{"level2":"hello"}}"#);
    }

    #[test]
    fn test_list_insert() {
        let mut json = JSON::from_str(r#"{"level1": []}"#).unwrap();

        // insert to empty array
        let operation_comp =
            OperationComponent::from_str(r#"{"p":["level1", 0], "li":{"hello":[1]}}"#).unwrap();
        json.apply(vec![vec![operation_comp]]).unwrap();
        assert_eq!(json.to_string(), r#"{"level1":[{"hello":[1]}]}"#);

        // insert to array
        let operation_comp =
            OperationComponent::from_str(r#"{"p":["level1", 0], "li":1}"#).unwrap();
        json.apply(vec![vec![operation_comp]]).unwrap();
        assert_eq!(json.to_string(), r#"{"level1":[1,{"hello":[1]}]}"#);

        // insert to inner array
        let operation_comp =
            OperationComponent::from_str(r#"{"p":["level1", 1, "hello",1], "li":[7,8]}"#).unwrap();
        json.apply(vec![vec![operation_comp]]).unwrap();
        assert_eq!(json.to_string(), r#"{"level1":[1,{"hello":[1,[7,8]]}]}"#);

        // append
        let operation_comp =
            OperationComponent::from_str(r#"{"p":["level1", 10], "li":[2,3]}"#).unwrap();
        json.apply(vec![vec![operation_comp]]).unwrap();
        assert_eq!(
            json.to_string(),
            r#"{"level1":[1,{"hello":[1,[7,8]]},[2,3]]}"#
        );
    }

    #[test]
    fn test_list_delete() {
        let origin_json = JSON::from_str(r#"{"level1":[1,{"hello":[1,[7,8]]}]}"#).unwrap();

        // delete from innser array
        let mut json = origin_json.clone();
        let operation_comp =
            OperationComponent::from_str(r#"{"p":["level1", 1, "hello", 1], "ld":[7,8]}"#).unwrap();
        json.apply(vec![vec![operation_comp]]).unwrap();
        assert_eq!(json.to_string(), r#"{"level1":[1,{"hello":[1]}]}"#);

        // delete from inner object
        let mut json = origin_json.clone();
        let operation_comp =
            OperationComponent::from_str(r#"{"p":["level1", 1], "ld":{"hello":[1,[7,8]]}}"#)
                .unwrap();
        json.apply(vec![vec![operation_comp]]).unwrap();
        assert_eq!(json.to_string(), r#"{"level1":[1]}"#);
    }

    #[test]
    fn test_list_replace() {
        let origin_json = JSON::from_str(r#"{"level1":[1,{"hello":[1,[7,8]]}]}"#).unwrap();

        // replace from innser array
        let mut json = origin_json.clone();
        let operation_comp = OperationComponent::from_str(
            r#"{"p":["level1", 1, "hello", 1], "li":{"hello":"world"}, "ld":[7,8]}"#,
        )
        .unwrap();
        json.apply(vec![vec![operation_comp]]).unwrap();
        assert_eq!(
            json.to_string(),
            r#"{"level1":[1,{"hello":[1,{"hello":"world"}]}]}"#
        );

        // replace from inner object
        let mut json = origin_json.clone();
        let operation_comp = OperationComponent::from_str(
            r#"{"p":["level1", 1], "li": {"hello":"world"}, "ld":{"hello":[1,[7,8]]}}"#,
        )
        .unwrap();
        json.apply(vec![vec![operation_comp]]).unwrap();
        assert_eq!(json.to_string(), r#"{"level1":[1,{"hello":"world"}]}"#);
    }

    #[test]
    fn test_list_move() {
        let origin_json = JSON::from_str(r#"{"level1":[1,{"hello":[1,[7,8], 9, 10]}]}"#).unwrap();

        // move left
        let mut json = origin_json.clone();
        let operation_comp =
            OperationComponent::from_str(r#"{"p":["level1", 1, "hello", 2], "lm":1}"#).unwrap();
        json.apply(vec![vec![operation_comp]]).unwrap();
        assert_eq!(
            json.to_string(),
            r#"{"level1":[1,{"hello":[1,9,[7,8],10]}]}"#
        );

        // move right
        let mut json = origin_json.clone();
        let operation_comp =
            OperationComponent::from_str(r#"{"p":["level1", 1, "hello", 1], "lm":2}"#).unwrap();
        json.apply(vec![vec![operation_comp]]).unwrap();
        assert_eq!(
            json.to_string(),
            r#"{"level1":[1,{"hello":[1,9,[7,8],10]}]}"#
        );

        // stay put
        let mut json = origin_json.clone();
        let operation_comp =
            OperationComponent::from_str(r#"{"p":["level1", 1, "hello", 1], "lm":1}"#).unwrap();
        json.apply(vec![vec![operation_comp]]).unwrap();
        assert_eq!(
            json.to_string(),
            r#"{"level1":[1,{"hello":[1,[7,8],9,10]}]}"#
        );
    }
}
