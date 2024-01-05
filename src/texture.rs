pub use crate::lib::*;

/// General purpose errors returned by this module.
#[derive(Error, Debug)]
enum TextureError {
    #[error("blending factor `{0}` not in (0..100) range")]
    BlendingOutOfRange(u8),
    #[error("steps don't cover full range (0..255) or they overflow it")]
    PartialRange,
}

/// Available error values returned by `Step::TryFrom`
#[derive(Error, Debug)]
enum StepError {
    #[error("malformed step description `{0}`")]
    ParseError(String),
}

/// Deserialized `$tile-name:$X..$Y` string.
#[derive(Debug)]
struct Step<'a> {
    name: &'a str,
    range: RangeInclusive<u8>,
}

impl<'a> Display for Step<'a> {
    /// Reproduces identical string that was provided during object construction.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}..{}", self.name, self.range.start(), self.range.end())
    }
}

/// Creates a `Step` from `$tile-name:$X..$Y` string.
impl<'a> TryFrom<&'a str> for Step<'a> {
    type Error = StepError;
    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        let [tile, range] = value.split(':').collect::<Vec<&str>>()[..] else {
            return Err(StepError::ParseError(value.to_owned()));
        };

        let [start, end] = range.split("..").collect::<Vec<&str>>()[..] else {
            return Err(StepError::ParseError(value.to_owned()));
        };

        let start = match start.parse::<u8>() {
            Ok(v) => v,
            Err(_) => return Err(StepError::ParseError(value.to_owned())),
        };

        let end = match end.parse::<u8>() {
            Ok(v) => v,
            Err(_) => return Err(StepError::ParseError(value.to_owned())),
        };

        Ok(Self {
            name: tile,
            range: RangeInclusive::new(start, end),
        })
    }
}

/// As `Step` holds a single threshold, the `Series` holds all possible
/// thresholds.
#[derive(Debug)]
struct Series<'a> {
    series: Vec<Step<'a>>,
    rng: SmallRng,
    blending: RangeInclusive<u8>,
    seed: u64,
}

impl<'a> TryFrom<Vec<Step<'a>>> for Series<'a> {
    type Error = TextureError;
    fn try_from(value: Vec<Step<'a>>) -> Result<Self, Self::Error> {
        // Check if all provided steps cumulatively exhaust u8 range.
        let range = value.iter()
            .fold(u8::MAX, |sum, v|
                  sum.wrapping_sub(v.range.end() - v.range.start() + 1)
            );

        // If range is fully covered, sum will underflow and wrap around.
        if range != u8::MAX {
            return Err(TextureError::PartialRange)
        }

        let seed = thread_rng().gen_range(1..u64::MAX);
        Ok(Self {
            series: value,
            rng: SmallRng::seed_from_u64(seed),
            blending: RangeInclusive::new(0, 0),
            seed
        })
    }
}

impl<'a> Display for Series<'a> {
    /// Reconstruct entire chain of steps as provided as input string.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = String::new();
        for step in self.series.iter() {
            s.push_str(&step.to_string());
            s.push(' ')
        }

        write!(f, "{}", &s)
    }
}

impl<'a> Series<'a> {
    /// Checks into which threshold/range given `value` falls in.
    /// Once the reference to Step is found, return it's index/activation value.
    /// NOTE: The hard assumption is that `Vec<Step>` exhausts entire `u8` width.
    fn activate(&mut self, mut value: u8) -> u8 {
        if !self.blending.is_empty() {
            // If it overflows clamp it at u8::MAX
            value = value.saturating_add(self.rng.gen_range(self.blending.clone()));
        }

        let (index, _) = self.series.iter()
            .enumerate()
            .find(|&(_, v)| v.range.contains(&value))
            .unwrap();
        // Lua is 1-based.
        (index + 1) as u8
    }

    /// Applies blending onto values passed to `Self::activate`
    fn with_blending(mut self, seed: u64, factor: u8) -> Self {
        // If seed == 0, then we already have default instance in Self::from.
        if seed != 0 {
            self.rng = SmallRng::seed_from_u64(seed);
            self.seed = seed;
        }

        // Map % value into [0..255 range]. Cast to u16 is required to deal
        // with temporary overflow.
        let factor: u16 = u16::from(factor) * u16::from(u8::MAX);
        let factor: f32 = f32::from(factor) * 0.01;
        self.blending = RangeInclusive::new(0, factor.floor() as u8);

        self
    }

    fn seed(&self) -> u64 {
        self.seed
    }

    /// Reverses operation performed in `Self::with_blending` and returns original
    /// value.
    fn blending(&self) -> u8 {
        let end: f32 = (*self.blending.end()).into();
        let mut factor: f32 = end / f32::from(u8::MAX);
        factor *= 100.0;
        factor = factor.ceil();

        factor as u8
    }

    /// Dump a lua array representating a map of activation values and tile names.
    fn as_lua_map(&self) -> String {
        let mut s = String::from("{");
        for (index, step) in self.series.iter().enumerate() {
            s.push_str(
                &format!("[{}]=\"{}\",", (index + 1), step.name)
            )
        }
        s.push('}');

        s
    }
}

/// Represents a 2D grid. Each cell holding a `T` type.
type Grid<T> = Vec<Vec<T>>;

/// Loads an image, converts into grayscale and iterates over each
/// pixel. The value within [0..255] is stored in 2D grid for further
/// processing.
fn load_grayscale<P: AsRef<Path>>(path: P) -> Result<Grid<u8>> {
    let mut grid: Grid<u8> = vec![];
    let img = image::open(path)?.to_luma8();

    for (_, row) in img.enumerate_rows() {
        grid.push(vec![]);
        let s = grid.last_mut().unwrap();
        for pixel in row {
            // Grab a grayscale value and push into vector.
            s.push(pixel.2.0[0]);
        }
    }

    Ok(grid)
}

/// Generates a comment with additional information.
fn header_lua(series: &Series) -> String {
    let mut s = String::from("-- bb");
    s.push_str(
        &format!(" --seed {}", series.seed().to_string())
    );
    s.push_str(
        &format!(" --blending {}", series.blending())
    );
    s.push_str(
        &format!(" --steps {}", series.to_string())
    );

    s
}

/// Converts `Grid<u8>` and `Series` into lua code. The basic structure is as follows.
///
/// ```lua
/// local mod = {};
/// mod.width = 512;
/// mod.height = 512;
/// mod.map = {
///     [n + 1] = "tile-name-0",
///     [n + 2] = "tile-name-1",
///     [n + 3] = "tile-name-special",
///     [n + m] = "...",
/// };
/// mod.grid = {
///     {n + 1, n + 3, n + 2, ..., n + m},
///     {n + 1, n + 3, n + 2, ..., n + m},
///     {...},
/// };
/// return mod
/// ```
///
/// The `map` is just a basic 1-based array that points at various factorio tile names.
/// The 'grid' is a clever way to represent a 2D texture in the memory. Think of it as
/// basic 2D array with columns (x) and rows (y). Any in-game position can be computed
/// using modulo and width/height to get offset into grid. Under an offset a value is
/// read that later used in `map` to extract reference to a tile name.
fn to_lua(grid: &Grid<u8>, series: &Series) -> String {
    let width = grid.len();
    let height = grid[0].len();

    let mut code = String::from(
        &format!("{}\n", header_lua(series))
    );
    code.push_str("local mod={};");
    code.push_str(
        &format!("mod.width={};", width)
    );
    code.push_str(
        &format!("mod.height={};", height)
    );
    code.push_str(
        &format!("mod.map={};", series.as_lua_map())
    );
    code.push_str("mod.grid={");
    for row in grid {
        code.push('{');
        for tile_id in row {
            let tile_id = tile_id.to_string();
            code.push_str(&tile_id);
            code.push(',')
        }
        code.push_str("},");
    }
    code.push_str("};return mod");
    code
}

/// Use supplied arguments to generate representation of a image texture as a lua array.
/// This function requires `step` and `image` input parameters.
/// Each `step` must adhere to the following format: `$tile-name:$X..$Y`
/// The `$` sign denotates dynamic input. The `$tile-name` is a direct name of tile
/// accessible in factorio engine, the `$X..$Y` is an inclusive range `[0..255]` where
/// a tile has to appear.
///
/// # Example
///
/// `dirt-1:0..25 dirt-2:26..51`
/// This will instruct the function to place tiles at those specific thresholds of a
/// grayscale derived from the image.
pub fn handle(args: &ArgMatches) -> Result<()> {
    // Iterate over each step string and convert into Step struct.
    let steps = args.get_many::<String>("steps").unwrap()
        .map(|v| v.as_str().try_into())
        .collect::<Result<Vec<Step>, _>>()?;
    // Get the blending factor.
    let blend = args.get_one::<String>("blending").unwrap()
        .parse::<u8>()?;
    if blend > 100 {
        bail!(TextureError::BlendingOutOfRange(blend))
    }
    // Then convert into Series.
    let seed = args.get_one::<String>("seed").unwrap()
        .parse::<u64>()?;
    let mut series = Series::try_from(steps)?
        .with_blending(seed, blend);
    // Load and convert image into grayscale, iterate over each pixel and convert
    // it into activation value. The activation value here means just an index of
    // a Step held in Series.
    let path = args.get_one::<String>("image").unwrap();
    let grid: Grid<u8> = load_grayscale(path)?.
        into_iter().map(
            |v| v.into_iter().map(
                |v| series.activate(v)
            ).collect()
        ).collect();
    // Now combine information from grid and series to compute final lua output.
    let code = to_lua(&grid, &series);

    let path = args.get_one::<String>("output").unwrap();
    File::create(path)?.write_all(code.as_bytes())?;

    Ok(())
}
