use std::sync::{Mutex};
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

/// 日志适配器，根据当前模式选择输出方式
pub struct LogAdapter;

impl LogAdapter {
    /// 初始化日志系统
    pub fn init() -> Result<(), Box<dyn std::error::Error>> {
        // 在终端模式下，使用env_logger
        env_logger::Builder::from_default_env()
            .filter_level(log::LevelFilter::Info)
            .init();
        
        // 同时初始化tui_logger，但它不会成为主要的日志器
        // 当我们切换到表格模式时，会重新初始化
        Ok(())
    }

    /// 重新初始化为表格模式
    pub fn init_table_mode() -> Result<(), Box<dyn std::error::Error>> {
        // 初始化tui_logger作为主要日志器
        tui_logger::init_logger(log::LevelFilter::Info)?;
        tui_logger::set_default_level(log::LevelFilter::Info);
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
        // 注意：这里不重新初始化日志器，因为可能会导致问题
        // 在实际使用中，模式切换主要影响表格显示的行为
    }

    /// 切换到表格模式
    pub fn switch_to_table() {
        Self::set_mode(LogMode::Table);
        // 尝试重新初始化为tui_logger模式
        Self::init_table_mode().ok();
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