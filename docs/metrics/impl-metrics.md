# Impl Metrics

Impl metrics measure the structure and cohesion of `impl` blocks (Rust's equivalent of classes). These help identify god classes, interface pollution, and lack of cohesion.

## Metrics

### Methods per Impl

Counts the number of methods in each impl block.

| Statistic | Description |
|-----------|-------------|
| max | Most methods in any impl |
| mean | Average methods per impl |
| p90 | 90th percentile |

**Default thresholds:**

| Bucket | Threshold |
|--------|-----------|
| Excellent | < 5 methods |
| Good | < 10 methods |
| Fair | < 15 methods |
| Poor | < 25 methods |
| Critical | >= 25 methods |

**Why it matters:** Impls with many methods often indicate a "god class" that has too many responsibilities. Consider splitting into multiple focused types.

### Traits per Type

Counts how many traits each type implements.

| Statistic | Description |
|-----------|-------------|
| max | Most traits on any type |
| mean | Average traits per type |

**Default thresholds:**

| Bucket | Threshold |
|--------|-----------|
| Excellent | < 3 traits |
| Good | < 5 traits |
| Fair | < 8 traits |
| Poor | < 12 traits |
| Critical | >= 12 traits |

**Why it matters:** Types implementing many traits may have unclear responsibilities or be trying to satisfy too many interfaces. This can indicate interface pollution.

### LCOM4 (Lack of Cohesion of Methods)

Measures how cohesive an impl is by counting connected components in a method graph.

LCOM4 builds an undirected graph where:
- **Nodes** are methods of the struct
- **Edges** connect two methods if they share access to a common field OR one calls the other

LCOM4 = the number of connected components in this graph.

- **1** = All methods are related (cohesive)
- **2+** = The struct has unrelated groups of methods and could be split

| Statistic | Description |
|-----------|-------------|
| max | Highest LCOM4 (least cohesive impl) |
| mean | Average LCOM4 across impls |

**Default thresholds:**

| Bucket | Threshold |
|--------|-----------|
| Excellent | < 2 |
| Good | < 3 |
| Fair | < 4 |
| Poor | < 5 |
| Critical | >= 5 |

**Why it matters:** LCOM4 >= 2 means the struct contains unrelated groups of methods that don't share state or call each other. Each connected component could potentially be its own struct.

## Example Output

```
Impl Metrics
============

Methods/Impl: max=17, mean=2.1, p90=5
Traits/Type:  max=2, mean=1.1
LCOM4:        max=3, mean=1.2

Largest Impls:
  impl CacheStore (17 methods)
  impl SemanticTags (7 methods)

Types with Many Traits:
  Config (3 traits)

Least Cohesive Impls (LCOM4 >= 2):
  impl ComplexityMetrics (LCOM4=3, 3 connected components)
  impl CacheStore (LCOM4=2, 2 connected components)
```

## Interpretation

- **Large impls (many methods)**: Consider the Single Responsibility Principle. Can this be split into multiple focused types?
- **Many traits per type**: Is this type trying to do too much? Could some traits be combined or the type split?
- **LCOM4 >= 2**: The impl has disconnected groups of methods. Either:
  - The impl should be split into cohesive groups (one per connected component)
  - Methods are stateless utilities (which is fine)
  - Field tracking may be incomplete (check if methods access fields through nested calls)

## Configuration

```yaml
thresholds:
  methods_per_impl:
    excellent: 5
    good: 10
    fair: 15
    poor: 25

  traits_per_type:
    excellent: 3
    good: 5
    fair: 8
    poor: 12

  lcom:
    excellent: 2
    good: 3
    fair: 4
    poor: 5
```

## Method Connectivity Tracking

LCOM4 connects methods that share field access or call each other. The extractor tracks:

- `self.field` read access
- `self.field = value` write access
- Method-to-method calls within the same struct

Limitations:
- Field access through nested method calls is not tracked
- Field access in closures may not be attributed correctly
