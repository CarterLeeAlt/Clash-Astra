//! 窗口内容区使用逻辑像素；创建、状态恢复和保存共用同一组尺寸约束。

pub const MIN_WINDOW_WIDTH: f64 = 1100.0;
pub const MIN_WINDOW_HEIGHT: f64 = 700.0;
pub const DEFAULT_WINDOW_WIDTH: f64 = MIN_WINDOW_WIDTH;
pub const DEFAULT_WINDOW_HEIGHT: f64 = MIN_WINDOW_HEIGHT;
const MAX_WINDOW_DIMENSION: f64 = 20_000.0;
const MAX_WINDOW_POSITION: f64 = 100_000.0;

/// 保存的窗口状态依次为宽、高、横坐标和纵坐标，统一按当前尺寸约束校验。
pub fn valid_saved_geometry(geometry: &[f64]) -> bool {
    geometry.len() == 4
        && geometry.iter().all(|value| value.is_finite())
        && (MIN_WINDOW_WIDTH..=MAX_WINDOW_DIMENSION).contains(&geometry[0])
        && (MIN_WINDOW_HEIGHT..=MAX_WINDOW_DIMENSION).contains(&geometry[1])
        && geometry[2].abs() <= MAX_WINDOW_POSITION
        && geometry[3].abs() <= MAX_WINDOW_POSITION
}

/// 最大化时保留上次普通窗口尺寸；拒绝保存不能在下次启动恢复的几何数据。
pub fn geometry_to_persist(
    width: f64,
    height: f64,
    x: f64,
    y: f64,
    maximized: bool,
) -> Option<Vec<f64>> {
    let geometry = [width, height, x, y];
    (!maximized && valid_saved_geometry(&geometry)).then(|| geometry.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_and_minimum_are_1100_by_700_logical_pixels() {
        assert_eq!(
            (DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT),
            (1100.0, 700.0)
        );
        assert_eq!((MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT), (1100.0, 700.0));
        assert!(valid_saved_geometry(&[1100.0, 700.0, 0.0, 0.0]));
        assert_eq!(
            geometry_to_persist(1100.0, 700.0, 0.0, 0.0, false),
            Some(vec![1100.0, 700.0, 0.0, 0.0])
        );
    }

    #[test]
    fn under_minimum_sizes_are_neither_restored_nor_saved() {
        for (width, height) in [
            (1099.0, 700.0),
            (1100.0, 699.0),
            (1099.99, 700.0),
            (1100.0, 699.99),
        ] {
            assert!(!valid_saved_geometry(&[width, height, 0.0, 0.0]));
            assert!(geometry_to_persist(width, height, 0.0, 0.0, false).is_none());
        }
    }

    #[test]
    fn valid_larger_geometry_and_negative_monitor_position_are_preserved() {
        let geometry = vec![1280.0, 800.0, -1200.0, -50.0];
        assert!(valid_saved_geometry(&geometry));
        assert_eq!(
            geometry_to_persist(1280.0, 800.0, -1200.0, -50.0, false),
            Some(geometry)
        );
        assert!(geometry_to_persist(1920.0, 1080.0, 0.0, 0.0, true).is_none());
    }

    #[test]
    fn malformed_and_unbounded_geometry_is_rejected() {
        for geometry in [
            vec![],
            vec![1100.0, 700.0],
            vec![1100.0, 700.0, f64::NAN, 0.0],
            vec![f64::INFINITY, 700.0, 0.0, 0.0],
            vec![20_001.0, 700.0, 0.0, 0.0],
            vec![1100.0, 700.0, 100_001.0, 0.0],
        ] {
            assert!(!valid_saved_geometry(&geometry));
        }
    }
}
