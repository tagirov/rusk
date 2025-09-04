use rusk::{Task, TaskManager};
use chrono::NaiveDate;
mod common;
use common::create_test_task;

#[test]
fn test_generate_next_id_empty() {
    let tm = TaskManager::new_empty().unwrap();
    let id = tm.generate_next_id().unwrap();
    assert_eq!(id, 1);
}

#[test]
fn test_generate_next_id_sequential() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![
        create_test_task(1, "Task 1", false),
        create_test_task(2, "Task 2", false),
        create_test_task(3, "Task 3", false),
    ];
    let id = tm.generate_next_id().unwrap();
    assert_eq!(id, 4);
}

#[test]
fn test_generate_next_id_with_gaps() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![
        create_test_task(1, "Task 1", false),
        create_test_task(3, "Task 3", false),
        create_test_task(5, "Task 5", false),
    ];
    let id = tm.generate_next_id().unwrap();
    assert_eq!(id, 2);
}

#[test]
fn test_generate_next_id_max_reached() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    // Fill up to 200 tasks (safe number)
    for i in 1..=200 {
        tm.tasks.push(Task {
            id: i,
            text: format!("Task {}", i),
            date: None,
            done: false,
        });
    }
    
    // Next ID should be 201
    let next_id = tm.generate_next_id().unwrap();
    assert_eq!(next_id, 201);
}

#[test]
fn test_find_task_by_id() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![
        create_test_task(1, "Task 1", false),
        create_test_task(2, "Task 2", false),
        create_test_task(3, "Task 3", false),
    ];
    
    assert_eq!(tm.find_task_by_id(1), Some(0));
    assert_eq!(tm.find_task_by_id(2), Some(1));
    assert_eq!(tm.find_task_by_id(3), Some(2));
    assert_eq!(tm.find_task_by_id(4), None);
}

#[test]
fn test_find_tasks_by_ids() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![
        create_test_task(1, "Task 1", false),
        create_test_task(2, "Task 2", false),
        create_test_task(3, "Task 3", false),
        create_test_task(4, "Task 4", false),
    ];
    
    let (found, not_found) = tm.find_tasks_by_ids(&[1, 3, 5]);
    assert_eq!(found, vec![0, 2]);
    assert_eq!(not_found, vec![5]);
}

#[test]
fn test_find_tasks_by_ids_empty() {
    let tm = TaskManager::new_empty().unwrap();
    let (found, not_found) = tm.find_tasks_by_ids(&[1, 2, 3]);
    assert!(found.is_empty());
    assert_eq!(not_found, vec![1, 2, 3]);
}

#[test]
fn test_add_task_success() {
    let mut tm = TaskManager::new_empty().unwrap();
    let text = vec!["Buy".to_string(), "groceries".to_string()];
    
    let result = tm.add_task(text, None);
    assert!(result.is_ok());
    assert_eq!(tm.tasks.len(), 1);
    assert_eq!(tm.tasks[0].id, 1);
    assert_eq!(tm.tasks[0].text, "Buy groceries");
    assert_eq!(tm.tasks[0].done, false);
}

#[test]
fn test_add_task_with_date() {
    let mut tm = TaskManager::new_empty().unwrap();
    let text = vec!["Meeting".to_string()];
    let date = Some("2025-01-15".to_string());
    
    let result = tm.add_task(text, date);
    assert!(result.is_ok());
    assert_eq!(tm.tasks.len(), 1);
    assert_eq!(tm.tasks[0].date, NaiveDate::parse_from_str("2025-01-15", "%Y-%m-%d").ok());
}

#[test]
fn test_add_task_empty_text() {
    let mut tm = TaskManager::new_empty().unwrap();
    let text = vec!["".to_string()];
    
    let result = tm.add_task(text, None);
    assert!(result.is_err());
    assert_eq!(tm.tasks.len(), 0);
}

#[test]
fn test_delete_tasks() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![
        create_test_task(1, "Task 1", false),
        create_test_task(2, "Task 2", false),
        create_test_task(3, "Task 3", false),
    ];
    
    let not_found = tm.delete_tasks(vec![1, 3]).unwrap();
    assert!(not_found.is_empty());
    assert_eq!(tm.tasks.len(), 1);
    assert_eq!(tm.tasks[0].id, 2);
}

#[test]
fn test_delete_tasks_not_found() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![
        create_test_task(1, "Task 1", false),
        create_test_task(2, "Task 2", false),
    ];
    
    let not_found = tm.delete_tasks(vec![1, 3, 5]).unwrap();
    // Sort both vectors for comparison since order doesn't matter
    let mut expected = vec![3, 5];
    expected.sort();
    let mut actual = not_found;
    actual.sort();
    assert_eq!(actual, expected);
    assert_eq!(tm.tasks.len(), 1);
    assert_eq!(tm.tasks[0].id, 2);
}

#[test]
fn test_delete_all_done() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![
        create_test_task(1, "Task 1", true),
        create_test_task(2, "Task 2", false),
        create_test_task(3, "Task 3", true),
    ];
    
    let deleted = tm.delete_all_done().unwrap();
    assert_eq!(deleted, 2);
    assert_eq!(tm.tasks.len(), 1);
    assert_eq!(tm.tasks[0].id, 2);
}

#[test]
fn test_delete_all_done_empty() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![
        create_test_task(1, "Task 1", false),
        create_test_task(2, "Task 2", false),
    ];
    
    let deleted = tm.delete_all_done().unwrap();
    assert_eq!(deleted, 0);
    assert_eq!(tm.tasks.len(), 2);
}

#[test]
fn test_mark_tasks() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![
        create_test_task(1, "Task 1", false),
        create_test_task(2, "Task 2", false),
        create_test_task(3, "Task 3", false),
    ];
    
    let (_marked, not_found) = tm.mark_tasks(vec![1, 3]).unwrap();
    assert!(not_found.is_empty());
    assert!(tm.tasks[0].done);
    assert!(!tm.tasks[1].done);
    assert!(tm.tasks[2].done);
}

#[test]
fn test_mark_tasks_not_found() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![
        create_test_task(1, "Task 1", false),
        create_test_task(2, "Task 2", false),
    ];
    
    let (_marked, not_found) = tm.mark_tasks(vec![1, 3, 5]).unwrap();
    assert_eq!(not_found, vec![3, 5]);
    assert!(tm.tasks[0].done);
    assert!(!tm.tasks[1].done);
}

#[test]
fn test_edit_tasks() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![
        create_test_task(1, "Task 1", false),
        create_test_task(2, "Task 2", false),
    ];
    
    let text = Some(vec!["New".to_string(), "text".to_string()]);
    let date = Some("2025-01-15".to_string());
    
    let (_edited, _unchanged, not_found) = tm.edit_tasks(vec![1, 2], text.clone(), date.clone()).unwrap();
    assert!(not_found.is_empty());
    assert_eq!(tm.tasks[0].text, "New text");
    assert_eq!(tm.tasks[1].text, "New text");
    assert_eq!(tm.tasks[0].date, NaiveDate::parse_from_str("2025-01-15", "%Y-%m-%d").ok());
    assert_eq!(tm.tasks[1].date, NaiveDate::parse_from_str("2025-01-15", "%Y-%m-%d").ok());
}

#[test]
fn test_edit_tasks_partial() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![
        create_test_task(1, "Task 1", false),
        create_test_task(2, "Task 2", false),
    ];
    
    let text = Some(vec!["New".to_string(), "text".to_string()]);
    
    let (_edited, _unchanged, not_found) = tm.edit_tasks(vec![1], text, None).unwrap();
    assert!(not_found.is_empty());
    assert_eq!(tm.tasks[0].text, "New text");
    assert_eq!(tm.tasks[1].text, "Task 2");
}

#[test]
fn test_edit_tasks_not_found() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![
        create_test_task(1, "Task 1", false),
    ];
    
    let text = Some(vec!["New".to_string(), "text".to_string()]);
    
    let (_edited, _unchanged, not_found) = tm.edit_tasks(vec![1, 3], text, None).unwrap();
    assert_eq!(not_found, vec![3]);
    assert_eq!(tm.tasks[0].text, "New text");
}
