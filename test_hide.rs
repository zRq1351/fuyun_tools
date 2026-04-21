    let _ = window.hide();
    if request.recenter && is_shrinking {
        move_window_top_center(&window, Some(width_logical as f64));
    }
    if need_resize {
        window.set_size(target_size).unwrap();
    }
    let _ = window.show();
