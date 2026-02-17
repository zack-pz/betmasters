use num_complex::Complex;
use num_traits::Num;

use crate::worker::types::Worker;

impl<T> Worker<T>
where
    T: Num + Clone + Default + PartialOrd + Send + Sync + TryFrom<f64> + TryFrom<u32>,
{
    /// Convierte un f64 al tipo genérico T.
    pub(crate) fn from_f64(v: f64) -> T {
        T::try_from(v)
            .ok()
            .expect("Error al convertir f64 a T")
    }

    /// Convierte un u32 al tipo genérico T.
    pub(crate) fn from_u32(v: u32) -> T {
        T::try_from(v)
            .ok()
            .expect("Error al convertir u32 a T")
    }

    pub fn compute_block(&self, width: u32, height: u32, start_row: u32, end_row: u32) -> Vec<Vec<f64>> {
        // Obtenemos los límites actuales del plano complejo
        let x_min = self.x_min.last().unwrap().clone();
        let x_max = self.x_max.last().unwrap().clone();
        let y_min = self.y_min.last().unwrap().clone();
        let y_max = self.y_max.last().unwrap().clone();

        // Convertimos dimensiones a T para los cálculos
        let width_t = Self::from_u32(width);
        let height_t = Self::from_u32(height);
        
        // Calculamos cuánto cambia X e Y por cada píxel (el "paso")
        let range_x = x_max - x_min.clone();
        let range_y = y_max - y_min.clone();
        
        let dx = range_x / width_t;
        let dy = range_y / height_t;

        // Calculamos cada fila de forma secuencial
        (start_row..end_row)
            .into_iter()
            .map(|row_idx| {
                // Determinamos la coordenada Y de esta fila
                let row_pos = Self::from_u32(row_idx);
                let current_y = y_min.clone() + row_pos * dy.clone();
                
                // Calculamos cada píxel (columna) de la fila
                (0..width)
                    .map(|col_idx| {
                        // Determinamos la coordenada X de este píxel
                        let col_pos = Self::from_u32(col_idx);
                        let current_x = x_min.clone() + col_pos * dx.clone();
                        
                        // Evaluamos el punto en el fractal de Mandelbrot
                        Self::evaluate(current_x, current_y.clone(), self.max_iters)
                    })
                    .collect::<Vec<f64>>()
            })
            .collect()
    }

    /// Evalúa un punto (x, y) en el conjunto de Mandelbrot.
    /// Retorna el número de iteraciones antes de que el punto escape.
    pub(crate) fn evaluate(x: T, y: T, max_iters: usize) -> f64 {
        let c = Complex::<T>::new(x, y);
        let mut z = Complex::<T>::default();
        
        let four = Self::from_f64(4.0);

        for i in 0..max_iters {
            // Si el cuadrado de la magnitud supera 4, el punto ha escapado
            if z.norm_sqr() > four {
                return i as f64;
            }
            
            // Aplicamos la función iterativa: z = z^2 + c
            z = z.clone() * z + c.clone();
        }

        // Si alcanzamos el máximo de iteraciones, el punto está en el conjunto
        max_iters as f64
    }
}
