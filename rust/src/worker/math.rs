use num_complex::Complex;
use num_traits::Num;

#[cfg(feature = "rayon")]
use rayon::prelude::*;

use crate::worker::types::Worker;

impl<T> Worker<T>
where
    T: Num + Clone + Default + PartialOrd + Send + Sync + TryFrom<f64> + TryFrom<u32>,
{
    pub(crate) fn from_f64(v: f64) -> T {
        T::try_from(v)
            .ok()
            .ok_or("conversion failed")
            .expect("Failed to convert from f64")
    }

    #[allow(dead_code)]
    pub(crate) fn from_u32(v: u32) -> T {
        T::try_from(v)
            .ok()
            .ok_or("conversion failed")
            .expect("Failed to convert from u32")
    }

    #[allow(dead_code)]
    pub(crate) fn remap(value: T, start1: T, stop1: T, start2: T, stop2: T) -> T {
        start2.clone()
            + (value - start1.clone()) * (stop2 - start2) / (stop1 - start1)
    }

    #[cfg(feature = "rayon")]
    pub fn compute_block(&self, width: u32, height: u32, start_row: u32, end_row: u32) -> Vec<Vec<f64>> {
        (start_row..end_row)
            .into_par_iter()
            .map(|v| {
                let y: T = Self::remap(
                    Self::from_u32(v),
                    Self::from_f64(0.0),
                    Self::from_u32(height),
                    self.y_min.last().unwrap().clone(),
                    self.y_max.last().unwrap().clone(),
                );
                (0..width)
                    .map(|u| {
                        let x: T = Self::remap(
                            Self::from_u32(u),
                            Self::from_f64(0.0),
                            Self::from_u32(width),
                            self.x_min.last().unwrap().clone(),
                            self.x_max.last().unwrap().clone(),
                        );
                        Self::evaluate(x.clone(), y.clone(), self.max_iters)
                    })
                    .collect()
            })
            .collect()
    }

    #[allow(dead_code)]
    pub(crate) fn evaluate(x: T, y: T, max_iters: usize) -> f64 {
        let q = (x.clone() - Self::from_f64(0.25)) * (x.clone() - Self::from_f64(0.25)) + y.clone() * y.clone();
        if q.clone() * (q + (x.clone() - Self::from_f64(0.25))) <= y.clone() * y.clone() * Self::from_f64(0.25) {
            return max_iters as f64;
        }
        let mut z = Complex::<T>::default();
        let mut z_old = Complex::<T>::default();
        let c = Complex::<T>::new(x, y);
        for i in 0..max_iters {
            if z.norm_sqr() >= Self::from_f64(4.0) {
                return i as f64 - 1.0;
            }
            z = z.clone() * z + c.clone();
            if z == z_old {
                return max_iters as f64;
            }
            if i % 20 == 0 {
                z_old = z.clone();
            }
        }
        max_iters as f64
    }
}
