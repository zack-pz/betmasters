use crate::worker::types::Worker;

impl Worker {
    pub fn compute_block(&self, width: u32, height: u32, start_row: u32, end_row: u32) -> Vec<u32> {
        // Obtenemos los límites actuales del plano complejo
        let x_min = self.x_min;
        let x_max = self.x_max;
        let y_min = self.y_min;
        let y_max = self.y_max;

        // Calculamos cuánto cambia X e Y por cada píxel (el "paso")
        let range_x = x_max - x_min;
        let range_y = y_max - y_min;
        
        let dx = range_x / width as f64;
        let dy = range_y / height as f64;

        let num_rows = end_row - start_row;
        let mut data = Vec::with_capacity((num_rows * width) as usize);

        // Calculamos cada fila de forma secuencial
        for row_idx in start_row..end_row {
            // Determinamos la coordenada Y de esta fila
            let current_y = y_min + row_idx as f64 * dy;
            
            // Calculamos cada píxel (columna) de la fila
            for col_idx in 0..width {
                // Determinamos la coordenada X de este píxel
                let current_x = x_min + col_idx as f64 * dx;
                
                // Evaluamos el punto en el fractal de Mandelbrot
                data.push(Self::evaluate(current_x, current_y, self.max_iters));
            }
        }
        data
    }

    // Evalúa un punto (x, y) en el conjunto de Mandelbrot.
    // Retorna el número de iteraciones antes de que el punto escape.
    pub(crate) fn evaluate(x: f64, y: f64, max_iters: usize) -> u32 {
        let mut a = 0.0;
        let mut b = 0.0;

        for i in 0..max_iters {
            let a2 = a * a;
            let b2 = b * b;
            
            // Si el cuadrado de la magnitud supera 4, el punto ha escapado (|z|^2 = a^2 + b^2)
            if a2 + b2 > 4.0 {
                return i as u32;
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
        max_iters as u32
    }
}
