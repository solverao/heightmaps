# Heightmap Generator

Generador procedural de heightmaps con interfaz gráfica construido en Rust con egui.

## Compilación y ejecución

```bash
cargo build --release
cargo run --release
```

> **WSL2:** la app usa el renderer OpenGL (glow) en lugar de wgpu, compatible con llvmpipe sin necesidad de Vulkan.

## Estructura del proyecto

```
src/
├── main.rs      — punto de entrada y declaración de módulos
├── types.rs     — enums y estructuras de datos compartidas
├── app.rs       — estado de la aplicación y lógica de generación
├── ui.rs        — interfaz egui (impl eframe::App)
└── view3d.rs    — renderer 3D por software (mesh con proyección oblicua)
```

## Pipeline de generación

Cada vez que un parámetro cambia, `generate()` ejecuta este pipeline en orden:

```
1. Noise sampling        — muestreo del ruido base (con domain warp opcional)
2. Layer blending        — mezcla de capas adicionales
3. Normalize             — normaliza el resultado a [0, 1]
4. Falloff map           — multiplica por un gradiente de isla (opcional)
5. Hydraulic erosion     — simulación de partículas de agua (opcional)
6. Thermal erosion       — derrumbe de pendientes por talud de reposo (opcional)
7. Gaussian blur         — suavizado gaussiano separable (opcional)
8. Percentile normalize  — recorta outliers y renormaliza (opcional)
9. Post-process          — operación final sobre los valores normalizados
```

---

## Parámetros de ruido

### Noise algorithm

Algoritmo base de ruido. Se combina con el fractal combiner.

| Algoritmo | Descripción |
|-----------|-------------|
| Perlin | Clásico. Bueno para terreno general. |
| Open Simplex | Variante de simplex sin patrones en cuadrícula. |
| Super Simplex | Mejora de Open Simplex, más suave. |
| Value | Interpolación de valores aleatorios. Aspecto más "blocky". |
| Worley / Cellular | Basado en distancia a puntos aleatorios. Produce celdas orgánicas. |

### Fractal combiner

Combina múltiples octavas del algoritmo de ruido para añadir detalle.

| Combinador | Descripción |
|------------|-------------|
| None (raw) | Una sola octava. Útil para capas simples. |
| fBm | Fractional Brownian Motion. El más natural para terreno. |
| Billow | fBm con valor absoluto. Produce colinas redondeadas. |
| Ridged Multi | Invierte los valles, crea crestas y bordes pronunciados. |
| Hybrid Multi | Mezcla fBm y Ridged. Terreno variado. |
| Basic Multi | Multiplicación de octavas. Contraste más duro. |

### Parámetros fractal

| Parámetro | Rango | Efecto |
|-----------|-------|--------|
| Seed | 0 – 2³² | Cambia el patrón manteniendo el mismo algoritmo. El dado genera uno aleatorio. |
| Octaves | 1 – 12 | Número de capas de detalle. Más octavas = más detalle pero más lento. |
| Frequency | 0.1 – 20 | Escala del patrón. Valores altos = características más pequeñas. |
| Lacunarity | 1.0 – 4.0 | Factor de escala entre octavas. 2.0 = cada octava duplica la frecuencia. |
| Persistence | 0.0 – 1.0 | Peso de cada octava. 0.5 = cada octava aporta la mitad de la anterior. |

### Offset X / Y

Desplazamiento manual en el espacio de ruido. Permite explorar distintas regiones del mapa infinito sin cambiar el seed.

---

## Domain Warp

Distorsiona las coordenadas de entrada `(x, y)` con dos ruidos fBm independientes antes de samplear el ruido principal.

```
x_warped = x + noise_x(x, y) * strength
y_warped = y + noise_y(x, y) * strength
```

Produce formas orgánicas complejas: costas irregulares, cuevas, penínsulas. Compatible con capas adicionales y seamless.

| Parámetro | Efecto |
|-----------|--------|
| Strength | Magnitud de la distorsión. 0 = sin efecto, 2.0 = distorsión extrema. |
| Warp frequency | Escala del ruido de distorsión. Baja = curvas grandes, alta = detalles pequeños. |

## Seamless

Hace que el heightmap sea tileable sin costuras. Usa la técnica de blend con 4 muestras desplazadas:

```
resultado(x, y) = blend bilineal de:
  noise(x,   y  )
  noise(x-1, y  )
  noise(x,   y-1)
  noise(x-1, y-1)
```

Con pesos smoothstep (`t²(3-2t)`) que garantizan continuidad C1 (valor y pendiente) en los bordes. Compatible con domain warp y capas adicionales. Útil para texturas que se repiten en un engine.

---

## Capas adicionales (Capa 2 y Capa 3)

Cada capa genera su propio ruido de forma independiente y lo mezcla con la capa base.

| Parámetro | Efecto |
|-----------|--------|
| Noise / Fractal | Algoritmo independiente para esta capa. |
| Blend mode | Cómo se mezcla con la capa inferior (ver tabla). |
| Weight | Intensidad de la mezcla (0 = invisible, 1 = mezcla completa). |
| Freq scale | Multiplicador de frecuencia relativo a la capa base. 2.0 = el doble de detalle. |
| Seed offset | Se suma al seed global para que esta capa tenga un patrón diferente. |

### Blend modes

| Modo | Fórmula | Uso típico |
|------|---------|------------|
| Add | `base + capa * weight` | Añadir detalle, elevar zonas. |
| Multiply | `base * (1 - w + capa * w)` | Suprimir zonas bajas, conservar estructura. |
| Max | `lerp(base, max(base, capa), w)` | Combinar cimas de dos mapas. |
| Min | `lerp(base, min(base, capa), w)` | Combinar valles de dos mapas. |
| Screen | `lerp(base, 1-(1-base)(1-capa), w)` | Aclarar sin sobreexponer. |

---

## Chunk mode

Genera heightmaps contiguos de un mundo infinito. Con el mismo seed y el mismo chunk size, los chunks adyacentes comparten borde continuo sin costuras.

| Parámetro | Efecto |
|-----------|--------|
| Chunk X / Y | Coordenada del chunk en el grid. Los botones ↑ ↓ ← → navegan. |
| Chunk size | Tamaño de cada chunk en espacio de ruido (0.25 – 4.0). Controla el zoom del contenido. |
| Offset calculado | Muestra el offset efectivo = `chunk_pos * chunk_size`. |

**Flujo de trabajo para mundos grandes:**
1. Definir seed y parámetros una sola vez.
2. Navegar con las flechas y exportar cada chunk.
3. En el engine, posicionar el chunk `(cx, cy)` en `(cx * tam_metros, cy * tam_metros)`.

> El chunk mode reemplaza el offset manual. Al desactivarlo vuelven los controles manuales de Offset X/Y.

---

## Falloff map

Multiplica el heightmap por un degradado que va de 1 (centro) a 0 (borde), ideal para crear islas.

| Parámetro | Efecto |
|-----------|--------|
| Forma | Círculo (distancia euclidiana) o Cuadrado (distancia Chebyshev). |
| Radio interior | Zona central plana donde el falloff vale 1.0. |
| Radio exterior | Distancia donde el falloff llega a 0.0. |
| Irregularidad de orilla | Dos ruidos Perlin deforman las coordenadas antes de medir la distancia, produciendo costas irregulares. |
| Frecuencia de orilla | Escala del ruido de irregularidad. Baja = bahías grandes, alta = detalles dentados. |
| Curva | Exponente sobre smoothstep. < 1 = degradado suave y amplio. > 1 = meseta plana con caída brusca. |

La transición entre inner y outer usa `smoothstep(t)^exponent`, lo que evita bordes duros.

---

## Erosión hidráulica

Simulación de partículas de agua que excavan valles y depositan sedimento. Produce cauces, planicies aluviales y erosión diferencial en laderas.

### Algoritmo (particle droplet)

Cada gota ejecuta hasta 64 pasos:

1. Nace en posición aleatoria con velocidad 1 y agua 1.
2. Calcula el gradiente de altura en su posición actual (interpolación bilineal 2×2).
3. Actualiza la dirección mezclando la dirección anterior (inercia) con el gradiente negativo.
4. Se mueve un paso en esa dirección.
5. Compara la capacidad de sedimento con el sedimento que carga:
   - Si puede cargar más → **erosiona**: extrae material del terreno.
   - Si lleva demasiado → **deposita**: deja sedimento en la posición actual.
6. El agua se evapora. Cuando llega a ~0, la gota muere.

Al finalizar, el mapa se renormaliza a [0, 1].

### Parámetros

| Parámetro | Rango | Efecto |
|-----------|-------|--------|
| Gotas | 1k – 150k | Más gotas = más detalle y más tiempo de procesado. |
| Inercia | 0 – 0.99 | 0 = la gota siempre gira hacia la pendiente (canales ramificados). 1 = avanza en línea recta. |
| Capacidad | 1 – 20 | Cuánto sedimento puede cargar una gota. Alto = erosión más profunda. |
| Deposición | 0.01 – 1 | Fracción depositada cuando la gota va lenta. Alto = depósitos más bruscos. |
| Velocidad de erosión | 0.01 – 1 | Agresividad del excavado. |
| Evaporación | 0.001 – 0.1 | Tasa de pérdida de agua. Bajo = gotas más largas (ríos más largos). |

---

## Erosión térmica

Simula el **talud de reposo**: el ángulo máximo de estabilidad de una pendiente. El material que supera ese ángulo se derrumba hacia las celdas vecinas. Produce acantilados con base suavizada, mesetas bien definidas y un aspecto geológico más realista que la erosión hidráulica sola.

### Algoritmo (talud de reposo)

En cada iteración, para cada celda `(x, y)`:

1. Calcula la diferencia de altura `Δh` con cada vecino en 4 direcciones.
2. Si `Δh > talud`, la pendiente es inestable.
3. Mueve `(Δh − talud) × fuerza × 0.5` de material hacia el vecino, distribuido proporcionalmente entre todos los vecinos inestables.

Se repite `iteraciones` veces. Al finalizar se renormaliza a [0, 1].

> Combinar erosión hidráulica (canales y valles) seguida de erosión térmica (suavizado de pendientes) produce resultados muy naturales.

### Parámetros

| Parámetro | Rango | Efecto |
|-----------|-------|--------|
| Talud | 0.01 – 0.5 | Ángulo de reposo. Bajo = terreno muy suave (dunas). Alto = solo aplana pendientes extremas (acantilados). |
| Iteraciones | 1 – 100 | Más iteraciones = efecto más pronunciado. 20–40 es un buen punto de partida. |
| Fuerza | 0.01 – 1.0 | Fracción de material transferido por iteración. Alto = convergencia rápida pero puede crear artefactos. |

---

## Gaussian blur

Suaviza el heightmap con un filtro gaussiano separable (dos pasadas 1D: horizontal + vertical). Útil para eliminar artefactos de la erosión o suavizar ruido de alta frecuencia antes de exportar.

El kernel tiene radio `ceil(σ × 3)` píxeles, cubriendo el 99.7% del área de la distribución. La implementación separable es O(n × r) en lugar de O(n × r²).

| Parámetro | Rango | Efecto |
|-----------|-------|--------|
| Sigma (σ) | 0.3 – 10.0 | Desviación estándar del kernel. Mayor σ = más desenfoque. El tamaño del kernel se muestra en tiempo real. |

---

## Normalizar por percentil

Recorta los valores extremos del mapa y renormaliza el rango resultante a [0, 1]. Útil cuando la erosión, el domain warp o capas adicionales generan outliers que comprimen el rango visible.

```
lo  = valor en el percentil bajo   (ej. 2%)
hi  = valor en el percentil alto   (ej. 98%)
v'  = clamp((v - lo) / (hi - lo), 0, 1)
```

| Parámetro | Rango | Efecto |
|-----------|-------|--------|
| Percentil bajo | 0 – 49% | Elimina los píxeles más oscuros del rango. Sube el suelo efectivo. |
| Percentil alto | 51 – 100% | Elimina los píxeles más claros del rango. Baja el techo efectivo. |

Con los valores por defecto (2% / 98%) se descartan los extremos más ruidosos y el contraste del resto se maximiza.

---

## Post-process

Operación aplicada pixel a pixel sobre el mapa normalizado, después de la erosión.

| Operación | Efecto |
|-----------|--------|
| None | Sin cambios. |
| Terrace / Posterize | Cuantiza las alturas en N niveles. Crea mesetas estilo Minecraft. |
| Power curve | `v^exp`. < 1 = aplana las cimas, > 1 = aplana los valles. |
| Invert | `1 - v`. Invierte el mapa (mares se vuelven montañas). |
| Abs (ridges) | `|v - 0.5| * 2`. Produce crestas simétricas donde antes había valles. |
| Clamp range | Recorta a [min, max] y renormaliza. Aislar una franja de alturas. |

---

## Preview

| Parámetro | Efecto |
|-----------|--------|
| Preview color | Paleta de colores de la vista 2D (ver tabla). |
| Preview resolution | Resolución de la textura del preview (64 – 512 px). No afecta la exportación. |

### Paletas de color

| Paleta | Descripción |
|--------|-------------|
| Grayscale | Escala de grises lineal. Equivale al archivo exportado. |
| Terrain | Agua profunda → poco profunda → arena → pasto → roca → nieve. |
| Heatmap | Azul oscuro → cian → amarillo → naranja → blanco. |

---

## Vista 3D

Render del terreno como mesh triangulado con proyección oblicua y sombreado Lambert. Usa `egui::Mesh` directamente para evitar restricciones de convexidad.

| Parámetro | Efecto |
|-----------|--------|
| Rotación | Gira el terreno 0 – 360°. El orden de dibujo (back-to-front) se ajusta automáticamente. |
| Escala vertical | Exagera la altura. 1.0 = proporcional, 3.0 = montañas muy pronunciadas. |
| Resolución 3D | Grid del mesh (16 – 128 px). Independiente de la resolución 2D. |

La resolución 3D es independiente del preview 2D — se puede tener un preview 2D de 256 px y una vista 3D de 48 px para que sea fluida. El color de la vista 3D respeta el ColorMode seleccionado.

**Sombreado:** gradiente de altura → normal aproximada por Sobel 2×2 → producto punto con la dirección de luz → luminancia Lambert (`0.38 + 0.62 * max(0, dot)`).

---

## Exportación

La ruta base se escribe en el campo de texto. Los botones derivan sus rutas del stem de esa ruta:

| Botón | Archivo generado | Descripción |
|-------|-----------------|-------------|
| 💾 8-bit | `<nombre>.png` | PNG escala de grises 8 bits (0 – 255). |
| 💾 16-bit | `<nombre>_16.png` | PNG escala de grises 16 bits (0 – 65535). Mayor precisión para engines. |
| 🗺 Normal map | `<nombre>_normal.png` | PNG RGB con la normal derivada del heightmap. |
| 📦 OBJ | `<nombre>.obj` | Mesh triangulado 3D importable en Blender, Unity, Unreal, etc. |

### Normal map

La normal se calcula con el filtro Sobel 3×3 sobre el heightmap a la resolución de exportación:

```
dx = Sobel horizontal
dy = Sobel vertical
N  = normalize(-dx * strength, -dy * strength, 1.0)
R  = (Nx + 1) / 2 * 255
G  = (Ny + 1) / 2 * 255
B  = (Nz + 1) / 2 * 255
```

El parámetro **Normal map strength** (1 – 32) escala la magnitud de los gradientes. Valores altos resaltan detalles finos, valores bajos producen normales más suaves.

### OBJ export

Genera un mesh triangulado con `res × res` vértices y `(res−1)² × 2` triángulos. Cada vértice:

```
v  x        y               z
   [0, 1]   h × escala_3D   [0, 1]
```

La escala vertical reutiliza el parámetro **Escala vertical** de la vista 3D. El parámetro **OBJ resolution** (32 – 512 px) es independiente de la resolución de exportación PNG para mantener el archivo manejable.

---

## Presets

Guarda y carga todos los parámetros de generación como un archivo JSON legible. Útil para compartir configuraciones o retomar un trabajo anterior.

| Campo | Descripción |
|-------|-------------|
| Preset path | Ruta del archivo `.json`. Por defecto `~/preset.json`. |
| 💾 Guardar preset | Escribe el estado actual a disco. |
| 📂 Cargar preset | Lee el archivo y aplica todos los parámetros; regenera automáticamente. |

El JSON incluye todos los parámetros de ruido, capas, erosión, falloff, post-process y visualización. Los campos de exportación (rutas, resolución de preview) no se guardan.

---

## Exportación por lotes de chunks

Disponible cuando **Chunk mode** está activo. Exporta un rango rectangular de chunks de una sola vez.

| Campo | Descripción |
|-------|-------------|
| X de / a | Rango de coordenadas de chunk en el eje X. |
| Y de / a | Rango de coordenadas de chunk en el eje Y. |
| 📦 Exportar lote | Genera todos los chunks del rango y los guarda en el mismo directorio que el export path. |

Cada archivo se nombra `<stem>_cx<X>_cy<Y>.png`. Para un rango de 3×3 centrado en el origen se generan 9 archivos PNG de 8 bits a la resolución de exportación configurada.

**Flujo de trabajo:**
1. Activar Chunk mode y ajustar Chunk size.
2. Definir seed y parámetros.
3. Establecer el rango de X/Y y pulsar **Exportar lote**.
4. En el engine, posicionar el chunk `(cx, cy)` en `(cx × tam, cy × tam)`.

---

## Randomize

El botón **🎲 Randomize** (esquina superior derecha del panel) aleatoriza todos los parámetros de generación de una vez:

- Seed, algoritmo de ruido, fractal combiner, octavas, frecuencia, lacunarity, persistence.
- Domain warp (40% de probabilidad).
- Post-process (50% desactivado, 50% modo aleatorio).
- Falloff map (35%, con geometría coherente inner < outer).
- Capas adicionales (30% cada una, con blend mode y pesos aleatorios).

Los parámetros de exportación, resolución, erosión y vista 3D no se tocan.

---

## Dependencias

| Crate | Versión | Uso |
|-------|---------|-----|
| eframe | 0.31 | Framework de ventana y loop principal (renderer: glow/OpenGL). |
| egui | 0.31 | Widgets de la interfaz. |
| egui_extras | 0.31 | Soporte de imágenes en egui. |
| image | 0.25 | Escritura de PNG 8-bit, 16-bit y RGB. |
| noise | 0.9 | Algoritmos de ruido procedural. |
| rand | 0.8 | RNG para seed aleatorio y posición de gotas de erosión. |
| serde | 1 | Serialización/deserialización para presets. |
| serde_json | 1 | Formato JSON para archivos de preset. |
