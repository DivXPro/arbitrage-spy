use std::sync::{Mutex, Once};
use log::{Log, Metadata, Record, Level};

/// 日志输出模式
#[derive(Debug, Clone, PartialEq)]
pub enum LogMode {
    /// 终端模式：直接输出到控制台
    Terminal,
    /// 表格模式：输出到tui_logger
    Table,
}

/// 全局日志模式状态
static LOG_MODE: Mutex<LogMode> = Mutex::new(LogMode::Terminal);
static INIT_ONCE: Once = Once::new();

/// 日志适配器，根据当前模式选择输出方式
pub struct LogAdapter;

impl LogAdapter {
    /// 初始化日志系统（统一使用tui_logger）
    pub fn init() -> Result<(), Box<dyn std::error::Error>> {
        INIT_ONCE.call_once(|| {
            // 统一使用tui_logger作为日志系统
            // 这样可以在表格模式和终端模式之间切换
            if let Err(e) = tui_logger::init_logger(log::LevelFilter::Info) {
                eprintln!("Failed to initialize tui_logger: {}", e);
                // 如果tui_logger初始化失败，回退到env_logger
                env_logger::Builder::from_default_env()
                    .filter_level(log::LevelFilter::Info)
                    .init();
            } else {
                tui_logger::set_default_level(log::LevelFilter::Info);
            }
        });
        Ok(())
    }

    /// 初始化为表格模式（已经使用tui_logger，无需重新初始化）
    pub fn init_table_mode() -> Result<(), Box<dyn std::error::Error>> {
        // 由于我们已经使用tui_logger作为主要日志系统，这里无需额外操作
        Ok(())
    }

    /// 设置日志模式
    pub fn set_mode(mode: LogMode) {
        if let Ok(mut current_mode) = LOG_MODE.lock() {
            *current_mode = mode.clone();
        }
    }

    /// 获取当前日志模式
    pub fn get_mode() -> LogMode {
        match LOG_MODE.lock() {
            Ok(mode) => mode.clone(),
            Err(_) => LogMode::Terminal, // 如果锁被污染，返回默认的终端模式
        }
    }

    /// 切换到终端模式
    pub fn switch_to_terminal() {
        Self::set_mode(LogMode::Terminal);
        // 在终端模式下，tui_logger仍然工作
        // 日志会被tui_logger收集，但不会显示在TUI中
    }

    /// 切换到表格模式
    pub fn switch_to_table() {
        Self::set_mode(LogMode::Table);
        // 由于我们已经使用tui_logger，无需重新初始化
        // 日志会自动显示在TUI的日志区域中
    }
}

/// 强制输出到终端的宏
#[macro_export]
macro_rules! terminal_log {
    ($level:ident, $($arg:tt)*) => {
        {
            // 直接使用println!输出到终端
            println!("[{}] {}", stringify!($level).to_uppercase(), format!($($arg)*));
        }
    };
}

/// 强制输出到表格日志区的宏
#[macro_export]
macro_rules! table_log {
    ($level:ident, $($arg:tt)*) => {
        {
            // 使用标准log宏，会被tui_logger捕获
            log::$level!($($arg)*);
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_mode_switching() {
        LogAdapter::switch_to_table();
        assert_eq!(LogAdapter::get_mode(), LogMode::Table);
        
        LogAdapter::switch_to_terminal();
        assert_eq!(LogAdapter::get_mode(), LogMode::Terminal);
    }
}