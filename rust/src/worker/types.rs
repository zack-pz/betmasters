pub struct Worker<T> {
    pub(crate) coordinator_url: String,
    pub x_min: Vec<T>,
    pub x_max: Vec<T>,
    pub y_min: Vec<T>,
    pub y_max: Vec<T>,
    pub max_iters: usize,
}
