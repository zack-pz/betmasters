use crate::worker::types::Worker;

impl Worker {
    pub fn compute_block(&self, width: u32, height: u32, start_row: u32, end_row: u32) -> Vec<Vec<f64>> {
        // Obtenemos los límites actuales del plano complejo
        let x_min = *self.x_min.last().unwrap();
        let x_max = *self.x_max.last().unwrap();
        let y_min = *self.y_min.last().unwrap();
        let y_max = *self.y_max.last().unwrap();

        // Calculamos cuánto cambia X e Y por cada píxel (el "paso")
        let range_x = x_max - x_min;
        let range_y = y_max - y_min;
        
        let dx = range_x / width as f64;
        let dy = range_y / height as f64;

        // Calculamos cada fila de forma secuencial
        (start_row..end_row)
            .into_iter()
            .map(|row_idx| {
                // Determinamos la coordenada Y de esta fila
                let current_y = y_min + row_idx as f64 * dy;
                
                // Calculamos cada píxel (columna) de la fila
                (0..width)
                    .map(|col_idx| {
                        // Determinamos la coordenada X de este píxel
                        let current_x = x_min + col_idx as f64 * dx;
                        
                        // Evaluamos el punto en el fractal de Mandelbrot
                        Self::evaluate(current_x, current_y, self.max_iters)
                    })
                    .collect::<Vec<f64>>()
            })
            .collect()
    }

    // Evalúa un punto (x, y) en el conjunto de Mandelbrot.
    // Retorna el número de iteraciones antes de que el punto escape.
    pub(crate) fn evaluate(x: f64, y: f64, max_iters: usize) -> f64 {
        let mut a = 0.0;
        let mut b = 0.0;

        for i in 0..max_iters {
            let a2 = a * a;
            let b2 = b * b;
            
            // Si el cuadrado de la magnitud supera 4, el punto ha escapado (|z|^2 = a^2 + b^2)
            if a2 + b2 > 4.0 {
                return i as f64;
            }
            
            // Aplicamos la función iterativa: z = z^2 + c
            // Re(z^2 + c) = a^2 - b^2 + x
            // Im(z^2 + c) = 2ab + y
            let next_a = a2 - b2 + x;
            let next_b = 2.0 * a * b + y;
            
            a = next_a;
            b = next_b;
        }

        // Si alcanzamos el máximo de iteraciones, el punto está en el conjunto
        max_iters as f64
    }
}
