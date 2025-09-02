use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame, Terminal,
};
use std::io::{self, Stdout};
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Clone, Debug)]
pub struct PairDisplay {
    pub rank: usize,
    pub pair: String,
    pub dex: String,
    pub price: String,
    pub change_24h: String,
    pub liquidity: String,
    pub last_update: String,
}

#[derive(Debug, Clone)]
pub enum DisplayMessage {
    UpdateData(Vec<PairDisplay>),
    Shutdown,
}

pub struct TableDisplay {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    receiver: mpsc::Receiver<DisplayMessage>,
}

impl TableDisplay {
    pub fn new(receiver: mpsc::Receiver<DisplayMessage>) -> Result<Self> {
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;
        
        Ok(Self {
            terminal,
            receiver,
        })
    }
    
    pub async fn start_display(&mut self) -> Result<()> {
        // 启用原始模式并进入备用屏幕
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        
        let mut current_pairs = Vec::new();
        
        // 显示初始空表格
        self.terminal.draw(|f| Self::render_ui_static(f, &current_pairs))?;
        
        loop {
            // 检查键盘输入
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.code == KeyCode::Char('c') && key.modifiers.contains(event::KeyModifiers::CONTROL) {
                        break;
                    }
                }
            }
            
            // 检查是否有新的显示消息
            if let Ok(message) = self.receiver.try_recv() {
                match message {
                    DisplayMessage::UpdateData(pairs) => {
                        current_pairs = pairs;
                        self.terminal.draw(|f| Self::render_ui_static(f, &current_pairs))?;
                    }
                    DisplayMessage::Shutdown => {
                        break;
                    }
                }
            }
            
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        
        // 恢复终端状态
        terminal::disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen)?;
        println!("表格显示已停止");
        
        Ok(())
    }
    
    fn render_ui_static(f: &mut Frame, pairs: &[PairDisplay]) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3), // 标题
                Constraint::Min(0),    // 表格
                Constraint::Length(3), // 提示信息
            ])
            .split(f.size());
        
        // 渲染标题
        let title = Paragraph::new("🚀 实时交易对监控")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(title, chunks[0]);
        
        // 渲染表格
        if !pairs.is_empty() {
            let header_cells = ["排名", "交易对", "DEX", "价格 (USD)", "24h变化", "流动性", "最后更新"]
                .iter()
                .map(|h| Cell::from(*h).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
            let header = Row::new(header_cells).height(1).bottom_margin(1);
            
            let rows = pairs.iter().map(|pair| {
                let cells = vec![
                    Cell::from(pair.rank.to_string()),
                    Cell::from(pair.pair.clone()),
                    Cell::from(pair.dex.clone()),
                    Cell::from(pair.price.clone()),
                    Cell::from(pair.change_24h.clone()).style(
                        if pair.change_24h.starts_with('+') {
                            Style::default().fg(Color::Green)
                        } else {
                            Style::default().fg(Color::Red)
                        }
                    ),
                    Cell::from(pair.liquidity.clone()),
                    Cell::from(pair.last_update.clone()),
                ];
                Row::new(cells).height(1)
            });
            
            let table = Table::new(rows, &[
                Constraint::Length(4),  // 排名
                Constraint::Length(12), // 交易对
                Constraint::Length(12), // DEX
                Constraint::Length(12), // 价格
                Constraint::Length(8),  // 24h变化
                Constraint::Length(10), // 流动性
                Constraint::Length(10), // 最后更新
            ])
            .header(header)
            .block(Block::default().borders(Borders::ALL).title("交易对数据"));
            f.render_widget(table, chunks[1]);
        } else {
            let no_data = Paragraph::new("等待数据加载...")
                .style(Style::default().fg(Color::Gray))
                .block(Block::default().borders(Borders::ALL).title("交易对数据"));
            f.render_widget(no_data, chunks[1]);
        }
        
        // 渲染提示信息
        let help = Paragraph::new("按 Ctrl+C 退出监控 | 数据实时更新中...")
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(help, chunks[2]);
    }
}