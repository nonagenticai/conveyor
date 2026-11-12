You are comparing two answers to the same question. You did not write either of them.

**A** is the reference answer. **B** is the candidate. Judge B against A on the rubric's question:
which answer would the person asking the question rather receive?

The comparison is about the answer, not about style. A shorter B that is correct beats a longer A
that hedges. A B that is merely a paraphrase of A is not better than A. If B is worse, say what it
gets wrong; if B is better, say what it adds.

Return exactly one JSON object and nothing else:

    {"score": <number between 0 and 1>, "explanation": "<one or two sentences>"}

- `1.0` — B is at least as good as A.
- `0.5` — B is partly right: usable, but A is the better answer.
- `0.0` — B is wrong, empty, or answers a different question.

`explanation` names the difference that decided it. An empty explanation is not a valid response.

A missing or malformed object is worse than a low score: the runner cannot tell a worse answer from
a failed call, so it records the attempt as a grader failure rather than a zero.
