use rusk::TaskManager;
mod common;
use common::create_test_task;

#[test]
fn test_mark_tasks_returns_marked_info() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![
        create_test_task(1, "Task 1", false),
        create_test_task(2, "Task 2", true),
        create_test_task(3, "Task 3", false),
    ];

    let (marked, not_found) = tm.mark_tasks(vec![1, 2, 3]).unwrap();

    // Should return info about what each task was marked as
    assert_eq!(marked.len(), 3);
    assert_eq!(marked[0], (1, true)); // Task 1: false -> true
    assert_eq!(marked[1], (2, false)); // Task 2: true -> false  
    assert_eq!(marked[2], (3, true)); // Task 3: false -> true
    assert!(not_found.is_empty());

    // Verify actual state changes
    assert!(tm.tasks[0].done); // Task 1 now done
    assert!(!tm.tasks[1].done); // Task 2 now undone
    assert!(tm.tasks[2].done); // Task 3 now done
}

#[test]
fn test_mark_tasks_with_not_found() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![create_test_task(1, "Task 1", false)];

    let (marked, not_found) = tm.mark_tasks(vec![1, 99]).unwrap();

    assert_eq!(marked.len(), 1);
    assert_eq!(marked[0], (1, true)); // Task 1 marked as done
    assert_eq!(not_found, vec![99]);

    assert!(tm.tasks[0].done);
}

#[test]
fn test_mark_tasks_toggle_behavior() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![create_test_task(1, "Task 1", false)];

    // Mark as done
    let (marked, _) = tm.mark_tasks(vec![1]).unwrap();
    assert_eq!(marked[0], (1, true));
    assert!(tm.tasks[0].done);

    // Mark again (should toggle back to undone)
    let (marked, _) = tm.mark_tasks(vec![1]).unwrap();
    assert_eq!(marked[0], (1, false));
    assert!(!tm.tasks[0].done);
}

#[test]
fn test_mark_tasks_empty_list() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![create_test_task(1, "Task 1", false)];

    let (marked, not_found) = tm.mark_tasks(vec![]).unwrap();

    assert!(marked.is_empty());
    assert!(not_found.is_empty());
    assert!(!tm.tasks[0].done); // Should remain unchanged
}

#[test]
fn test_mark_tasks_all_not_found() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![create_test_task(1, "Task 1", false)];

    let (marked, not_found) = tm.mark_tasks(vec![99, 100]).unwrap();

    assert!(marked.is_empty());
    assert_eq!(not_found, vec![99, 100]);
    assert!(!tm.tasks[0].done); // Should remain unchanged
}
