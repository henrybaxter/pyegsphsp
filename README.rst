pyegsphsp
=========

Read and write egsphsp (EGSnrc phase space files) from Python.

Requirements:
-------------

- Python 2.7, 3.x

Installation:
-------------

    pip install pyegsphsp



/*
    TODO

    - translate (--translate)
        --in-place -i
        -x (added to x)
        -y (added to y)
        not normalized or checked at all
    - reflect (--reflect)
        --in-place
        -x (component of unit vector)
        -y (component of unit vector)
        reflects around unit vector, so eg to reflect around y axis
        -x=0, y=1
        error if sum of squares is not 1 (not unit vector)
    - rotate (--rotate)
        - vector to rotate around is always z (since we can't control z values)
        - rotation is described by an angle theta
        - we use the a rotation matrix
    - combine (--combine)
        remove old (--delete)
        n files into n + 1th positional

*/

