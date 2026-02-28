#[derive(Debug, Clone)]
pub struct Worker {
    pub(crate) coordinator_url: String,
    pub(crate) max_iters: usize,
    pub(crate) x_min: Vec<f64>,
    pub(crate) x_max: Vec<f64>,
    pub(crate) y_min: Vec<f64>,
    pub(crate) y_max: Vec<f64>,
}
