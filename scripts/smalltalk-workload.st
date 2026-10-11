"Portable work: equivalent Squeak 1.1 bytecodes live in examples/smalltalk-workload.rs.
Run with a stock Squeak 6.0 Cog bundle: bin/squeak -headless IMAGE THIS_FILE.
Writes rekursiv-benchmark.csv beside the image; exits without saving the image.
Different batch lengths amortize the timer; compare seconds per completed call."
[
    | fibClass loopClass out |
    Utilities authorInitials: 'RB'.
    out := FileStream forceNewFileNamed: 'rekursiv-benchmark.csv'.
    fibClass := Object subclass: #RekursivFibBench
        instanceVariableNames: '' classVariableNames: '' poolDictionaries: '' category: 'Rekursiv-Benchmark'.
    fibClass compile: 'work: n
        ^ n < 2 ifTrue: [1] ifFalse: [(self work: n - 1) + (self work: n - 2)]'.
    loopClass := Object subclass: #RekursivLoopBench
        instanceVariableNames: '' classVariableNames: '' poolDictionaries: '' category: 'Rekursiv-Benchmark'.
    loopClass compile: 'work: n
        | sum i |
        sum := 0. i := 0.
        [i < n] whileTrue: [sum := sum + i. i := i + 1].
        ^ sum'.
    out nextPutAll: Smalltalk version; cr.
    { {fibClass new. 'fib'. 18. 4181}. {loopClass new. 'loop'. 10000. 49995000} } do: [:case |
        | receiver name n expected repetitions result start elapsed |
        receiver := case first. name := case second. n := case third. expected := case fourth.
        repetitions := 10000.
        10 timesRepeat: [receiver work: n].
        1 to: 3 do: [:trial |
            Smalltalk garbageCollect.
            start := Time millisecondClockValue.
            repetitions timesRepeat: [result := receiver work: n].
            elapsed := (Time millisecondClockValue - start) / 1000.0.
            result = expected ifFalse: [self error: 'Benchmark result mismatch'].
            out nextPutAll: 'case='; nextPutAll: name;
                nextPutAll: ', trial='; print: trial;
                nextPutAll: ', n='; print: n;
                nextPutAll: ', repetitions='; print: repetitions;
                nextPutAll: ', result='; print: result;
                nextPutAll: ', seconds='; print: elapsed;
                nextPutAll: ', seconds_per_call='; print: elapsed / repetitions; cr; flush]].
    out close
] on: Error do: [:error |
    | stream |
    stream := FileStream forceNewFileNamed: 'rekursiv-error.txt'.
    stream nextPutAll: error description; cr.
    stream close.
    Smalltalk quitPrimitive: 1].
Smalltalk quitPrimitive.
