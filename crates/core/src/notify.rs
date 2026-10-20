// Уведомления о новых оценках: снимок и его сравнение (без платформенных
// пушей/планировщиков).

use std::collections::{BTreeMap, HashMap};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::crypto::{CONTEXT_NOTIFY, FILE_GRADES_NOTIFY};
use crate::model::{Mark, SubjectMarks};
use crate::ports::SecureStore;

/// Предмет -> отсортированные «подписи» оценок.
pub type Snapshot = BTreeMap<String, Vec<String>>;

fn mark_sig(m: &Mark) -> String {
    format!("{}|{}|{}|{}", m.value, m.date, m.short_name, m.long_name)
}

pub fn build_snapshot(subjects: &[SubjectMarks]) -> Snapshot {
    let mut map: Snapshot = BTreeMap::new();
    for s in subjects {
        let sigs: Vec<String> = s
            .marks
            .iter()
            .filter(|m| m.value > 0)
            .map(mark_sig)
            .collect();
        map.entry(s.subject.clone()).or_default().extend(sigs);
    }
    for v in map.values_mut() {
        v.sort();
    }
    map
}

/// Разница как мультимножество: значения новых оценок по предметам.
pub fn diff_new(prev: &Snapshot, subjects: &[SubjectMarks]) -> Vec<(String, Vec<i32>)> {
    let mut result = Vec::new();
    for s in subjects {
        let mut prev_counts: HashMap<String, i32> = HashMap::new();
        if let Some(ps) = prev.get(&s.subject) {
            for sig in ps {
                *prev_counts.entry(sig.clone()).or_insert(0) += 1;
            }
        }
        let mut new_vals = Vec::new();
        for m in s.marks.iter().filter(|m| m.value > 0) {
            let sig = mark_sig(m);
            match prev_counts.get_mut(&sig) {
                Some(c) if *c > 0 => *c -= 1,
                _ => new_vals.push(m.value),
            }
        }
        if !new_vals.is_empty() {
            result.push((s.subject.clone(), new_vals));
        }
    }
    result
}

/// Обновить снимок, сохранив предметы, временно пропавшие из ответа.
pub fn merge_snapshot(mut curr: Snapshot, prev: Snapshot) -> Snapshot {
    for (subject, sigs) in prev {
        curr.entry(subject).or_insert(sigs);
    }
    curr
}

pub fn load_snapshot(store: &dyn SecureStore, path: &str) -> Option<Snapshot> {
    let json = store.load(path, FILE_GRADES_NOTIFY, CONTEXT_NOTIFY)?;
    serde_json::from_str(&json).ok()
}

pub fn save_snapshot(store: &dyn SecureStore, path: &str, snap: &Snapshot) {
    match serde_json::to_string(snap) {
        Ok(json) => {
            if let Err(e) = store.save(path, FILE_GRADES_NOTIFY, CONTEXT_NOTIFY, &json) {
                log::warn!("notify: не удалось сохранить снимок: {:?}", e);
            }
        }
        Err(e) => log::warn!("notify: не удалось сериализовать снимок: {:?}", e),
    }
}

/// id уведомления от времени: счётчик в памяти обнулялся бы на каждом
/// холодном старте воркера, и пуши затирали бы друг друга.
pub fn next_notify_id() -> i32 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    4200 + (secs % 10_000) as i32
}

pub fn notification_text(new_marks: &[(String, Vec<i32>)]) -> (String, String) {
    let total: usize = new_marks.iter().map(|(_, v)| v.len()).sum();
    let title = if total == 1 {
        "Новая оценка".to_string()
    } else {
        format!("Новые оценки: {}", total)
    };
    let text = new_marks
        .iter()
        .map(|(subject, vals)| {
            let vals = vals
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}: {}", subject, vals)
        })
        .collect::<Vec<_>>()
        .join("\n");
    (title, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(value: i32, date: &str, short: &str) -> Mark {
        Mark {
            value,
            weight: 1,
            short_name: short.into(),
            long_name: String::new(),
            date: date.into(),
            note: String::new(),
        }
    }

    #[test]
    fn detects_duplicate_marks_as_multiset() {
        let prev = build_snapshot(&[SubjectMarks {
            subject: "Алгебра".into(),
            average: 5.0,
            marks: vec![mark(5, "01.02.2026", "5")],
        }]);
        // Две пятёрки за тот же день: одна была, вторая новая.
        let curr = vec![SubjectMarks {
            subject: "Алгебра".into(),
            average: 5.0,
            marks: vec![mark(5, "01.02.2026", "5"), mark(5, "01.02.2026", "5")],
        }];
        assert_eq!(diff_new(&prev, &curr), vec![("Алгебра".to_string(), vec![5])]);
    }

    #[test]
    fn merge_keeps_subjects_missing_from_response() {
        let curr: Snapshot = [("A".to_string(), vec!["1".to_string()])].into_iter().collect();
        let prev: Snapshot = [
            ("A".to_string(), vec!["old".to_string()]),
            ("B".to_string(), vec!["keep".to_string()]),
        ]
        .into_iter()
        .collect();
        let merged = merge_snapshot(curr, prev);
        assert_eq!(merged["A"], vec!["1".to_string()]);
        assert_eq!(merged["B"], vec!["keep".to_string()]);
    }

    #[test]
    fn builds_notification_text() {
        let (title, text) = notification_text(&[
            ("Алгебра".into(), vec![5, 4]),
            ("Физика".into(), vec![3]),
        ]);
        assert_eq!(title, "Новые оценки: 3");
        assert_eq!(text, "Алгебра: 5, 4\nФизика: 3");
    }
}
