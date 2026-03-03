#[derive(Debug, Clone)]
pub struct Worker {
    pub(crate) coordinator_url: String,
    pub(crate) max_iters: usize,
    pub(crate) x_min: f64,
    pub(crate) x_max: f64,
    pub(crate) y_min: f64,
    pub(crate) y_max: f64,
}
